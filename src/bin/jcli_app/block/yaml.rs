use chain_addr::{Address, Discrimination};
use chain_core::property::HasMessages;
use chain_crypto::bech32::Bech32;
use chain_crypto::{Ed25519Extended, PublicKey};
use chain_impl_mockchain::{
    block::{Block, BlockBuilder, ConsensusVersion},
    certificate::{Certificate, SignatureRaw},
    config::{Block0Date, ConfigParam},
    fee::LinearFee,
    legacy::{self, OldAddress},
    message::{InitialEnts, Message},
    milli::Milli,
    setting::{SignedUpdateProposal, UpdateProposal, UpdateProposalWithProposer},
    transaction,
    value::Value,
};
use jormungandr_utils::serde::{self, SerdeAsString, SerdeLeaderId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Genesis {
    /// the initial configuration of the blockchain
    ///
    /// * the start date of the block 0;
    /// * the discrimination;
    /// * ...
    ///
    /// All that is static and does not need to have any update
    /// mechanism.
    blockchain_configuration: BlockchainConfiguration,
    initial_setting: Update,
    #[serde(default)]
    initial_funds: Vec<InitialUTxO>,
    #[serde(default)]
    initial_certs: Vec<InitialCertificate>,
    #[serde(default)]
    legacy_funds: Vec<LegacyUTxO>,
}

#[derive(Clone, Serialize, Deserialize)]
struct BlockchainConfiguration {
    block0_date: u64,
    #[serde(with = "serde::as_string")]
    discrimination: Discrimination,
    #[serde(with = "serde::as_string")]
    block0_consensus: ConsensusVersion,
    slots_per_epoch: Option<u64>,
    slot_duration: u8,
    epoch_stability_depth: Option<u32>,
    #[serde(default)]
    consensus_leader_ids: Vec<SerdeLeaderId>,
    consensus_genesis_praos_param_d: Option<SerdeAsString<Milli>>,
    consensus_genesis_praos_param_f: Option<SerdeAsString<Milli>>,
}

/// the initial configuration of the blockchain
///
/// This is the data tha may be updated but which needs
/// to have an initial value in the blockchain (or not)
#[derive(Clone, Serialize, Deserialize)]
struct Update {
    max_number_of_transactions_per_block: Option<u32>,
    bootstrap_key_slots_percentage: Option<u8>,
    allow_account_creation: Option<bool>,
    linear_fee: Option<InitialLinearFee>,
}

#[derive(Clone, Serialize, Deserialize)]
struct InitialLinearFee {
    coefficient: u64,
    constant: u64,
    certificate: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
struct InitialCertificate(#[serde(with = "serde::certificate")] Certificate);

#[derive(Clone, Serialize, Deserialize)]
struct InitialUTxO {
    #[serde(with = "serde::address")]
    pub address: Address,
    #[serde(with = "serde::value")]
    pub value: Value,
}

#[derive(Clone, Serialize, Deserialize)]
struct LegacyUTxO {
    pub address: OldAddress,
    #[serde(with = "serde::value")]
    pub value: Value,
}

type StaticStr = &'static str;

custom_error! {pub Error
    FirstBlock0MessageNotInit = "First message of block 0 is not initial",
    SecondBlock0MessageNotUpdateProposal = "Second message of block 0 is not update proposal",
    InitUtxoHasInput = "Initial UTXO has input",
    InitConfigParamMissing { name: StaticStr } = "Initial message misses parameter {name}",
    InitConfigParamDuplicate { name: StaticStr } = "Initial message contains duplicate parameter {name}",
}

impl Genesis {
    pub fn from_block(block: &Block) -> Result<Self, Error> {
        let mut messages = block.messages().peekable();

        let blockchain_configuration = match messages.next() {
            Some(Message::Initial(initial)) => BlockchainConfiguration::from_ents(initial)?,
            _ => return Err(Error::FirstBlock0MessageNotInit),
        };
        let initial_setting = match messages.next() {
            Some(Message::UpdateProposal(upd)) => Update::from_proposal(&upd.proposal.proposal),
            _ => return Err(Error::SecondBlock0MessageNotUpdateProposal),
        };
        let initial_funds = get_initial_utxos(&mut messages)?;
        let legacy_funds = get_legacy_utxos(&mut messages);
        let initial_certs = get_initial_certs(&mut messages);

        Ok(Genesis {
            blockchain_configuration,
            initial_setting,
            initial_funds,
            legacy_funds,
            initial_certs,
        })
    }

    pub fn to_block(&self) -> Block {
        let mut builder = BlockBuilder::new();

        builder.message(Message::Initial(
            self.blockchain_configuration.clone().to_ents(),
        ));
        builder.message(self.initial_setting.clone().to_message());

        builder.messages(
            self.to_initial_messages(
                self.initial_setting
                    .max_number_of_transactions_per_block
                    .unwrap_or(255) as usize,
            ),
        );
        builder.messages(
            self.to_legacy_messages(
                self.initial_setting
                    .max_number_of_transactions_per_block
                    .unwrap_or(255) as usize,
            ),
        );
        builder.messages(self.to_certificate_messages());
        builder.make_genesis_block()
    }

    fn to_initial_messages(&self, max_output_per_message: usize) -> Vec<Message> {
        let mut messages = Vec::new();
        let mut utxo_iter = self.initial_funds.iter();

        while let Some(utxo) = utxo_iter.next() {
            let mut outputs = Vec::with_capacity(max_output_per_message);
            outputs.push(transaction::Output {
                address: utxo.address.clone(),
                value: utxo.value,
            });

            while let Some(utxo) = utxo_iter.next() {
                outputs.push(transaction::Output {
                    address: utxo.address.clone(),
                    value: utxo.value,
                });
                if outputs.len() == max_output_per_message {
                    break;
                }
            }

            let transaction = transaction::AuthenticatedTransaction {
                transaction: transaction::Transaction {
                    inputs: Vec::new(),
                    outputs: outputs,
                    extra: transaction::NoExtra,
                },
                witnesses: Vec::new(),
            };
            messages.push(Message::Transaction(transaction));
        }

        messages
    }

    fn to_certificate_messages(&self) -> Vec<Message> {
        self.initial_certs
            .iter()
            .map(|cert| transaction::AuthenticatedTransaction {
                transaction: transaction::Transaction {
                    inputs: vec![],
                    outputs: vec![],
                    extra: cert.0.clone(),
                },
                witnesses: vec![],
            })
            .map(Message::Certificate)
            .collect()
    }

    fn to_legacy_messages(&self, max_output_per_message: usize) -> Vec<Message> {
        let mut messages = Vec::new();
        let mut utxo_iter = self.legacy_funds.iter();

        while let Some(utxo) = utxo_iter.next() {
            let mut outputs = Vec::with_capacity(max_output_per_message);
            outputs.push((utxo.address.clone(), utxo.value));

            while let Some(utxo) = utxo_iter.next() {
                outputs.push((utxo.address.clone(), utxo.value));
                if outputs.len() == max_output_per_message {
                    break;
                }
            }

            let declaration = legacy::UtxoDeclaration { addrs: outputs };

            messages.push(Message::OldUtxoDeclaration(declaration));
        }

        messages
    }
}

type PeekableMessages<'a> = std::iter::Peekable<<&'a Block as HasMessages<'a>>::Messages>;

fn get_initial_utxos<'a>(messages: &mut PeekableMessages<'a>) -> Result<Vec<InitialUTxO>, Error> {
    let mut vec = Vec::new();

    while let Some(Message::Transaction(transaction)) = messages.peek() {
        messages.next();
        if !transaction.transaction.inputs.is_empty() {
            return Err(Error::InitUtxoHasInput);
        }

        for output in transaction.transaction.outputs.iter() {
            let initial_utxo = InitialUTxO {
                address: output.address.clone(),
                value: output.value,
            };

            vec.push(initial_utxo);
        }
    }

    Ok(vec)
}
fn get_legacy_utxos<'a>(messages: &mut PeekableMessages<'a>) -> Vec<LegacyUTxO> {
    let mut vec = Vec::new();

    while let Some(Message::OldUtxoDeclaration(old_decls)) = messages.peek() {
        messages.next();
        for (address, value) in old_decls.addrs.iter() {
            let legacy_utxo = LegacyUTxO {
                address: address.clone(),
                value: value.clone(),
            };

            vec.push(legacy_utxo);
        }
    }

    vec
}
fn get_initial_certs<'a>(messages: &mut PeekableMessages<'a>) -> Vec<InitialCertificate> {
    let mut vec = Vec::new();

    while let Some(Message::Certificate(transaction)) = messages.peek() {
        messages.next();
        let cert = transaction.transaction.extra.clone();
        vec.push(InitialCertificate(cert));
    }

    vec
}

impl BlockchainConfiguration {
    fn from_ents(ents: &InitialEnts) -> Result<Self, Error> {
        use chain_impl_mockchain::config::ConfigParam;
        let mut block0_date = None;
        let mut discrimination = None;
        let mut block0_consensus = None;
        let mut slots_per_epoch = None;
        let mut slot_duration = None;
        let mut epoch_stability_depth = None;
        let mut consensus_leader_ids = vec![];
        let mut consensus_genesis_praos_param_d = None;
        let mut consensus_genesis_praos_param_f = None;

        for ent in ents.iter() {
            match ent {
                ConfigParam::Block0Date(param) => {
                    block0_date.replace(param.0).map(|_| "Block0Date")
                }
                ConfigParam::ConsensusVersion(param) => {
                    block0_consensus.replace(*param).map(|_| "ConsensusVersion")
                }
                ConfigParam::Discrimination(param) => {
                    discrimination.replace(*param).map(|_| "Discrimination")
                }
                ConfigParam::SlotsPerEpoch(param) => {
                    slots_per_epoch.replace(*param).map(|_| "SlotsPerEpoch")
                }
                ConfigParam::SlotDuration(param) => {
                    slot_duration.replace(*param).map(|_| "SlotDuration")
                }
                ConfigParam::EpochStabilityDepth(param) => epoch_stability_depth
                    .replace(*param)
                    .map(|_| "EpochStabilityDepth"),
                ConfigParam::ConsensusLeaderId(param) => {
                    consensus_leader_ids.push(SerdeLeaderId(param.clone()));
                    None
                }
                ConfigParam::ConsensusGenesisPraosParamD(param) => consensus_genesis_praos_param_d
                    .replace(SerdeAsString(*param))
                    .map(|_| "ConsensusGenesisPraosParamD"),
                ConfigParam::ConsensusGenesisPraosActiveSlotsCoeff(param) => {
                    consensus_genesis_praos_param_f
                        .replace(SerdeAsString(*param))
                        .map(|_| "ConsensusGenesisPraosParamF")
                }
            }
            .map(|name| Err(Error::InitConfigParamDuplicate { name }))
            .unwrap_or(Ok(()))?;
        }

        Ok(BlockchainConfiguration {
            block0_date: block0_date.ok_or(Error::InitConfigParamMissing { name: "Block0Date" })?,
            discrimination: discrimination.ok_or(Error::InitConfigParamMissing {
                name: "Discrimination",
            })?,
            block0_consensus: block0_consensus.ok_or(Error::InitConfigParamMissing {
                name: "Block0Consensus",
            })?,
            slots_per_epoch,
            slot_duration: slot_duration.ok_or(Error::InitConfigParamMissing {
                name: "SlotDuration",
            })?,
            epoch_stability_depth,
            consensus_leader_ids,
            consensus_genesis_praos_param_d,
            consensus_genesis_praos_param_f,
        })
    }

    fn to_ents(self) -> InitialEnts {
        // Generate warning or error for each unused field
        let BlockchainConfiguration {
            block0_date,
            discrimination,
            block0_consensus,
            slots_per_epoch,
            slot_duration,
            epoch_stability_depth,
            consensus_leader_ids,
            consensus_genesis_praos_param_d,
            consensus_genesis_praos_param_f,
        } = self;
        let mut initial_ents = InitialEnts::new();
        initial_ents.push(ConfigParam::Block0Date(Block0Date(block0_date)));
        initial_ents.push(ConfigParam::Discrimination(discrimination));
        initial_ents.push(ConfigParam::ConsensusVersion(block0_consensus));
        if let Some(slots_per_epoch) = slots_per_epoch {
            initial_ents.push(ConfigParam::SlotsPerEpoch(slots_per_epoch))
        }
        initial_ents.push(ConfigParam::SlotDuration(slot_duration));
        if let Some(epoch_stability_depth) = epoch_stability_depth {
            initial_ents.push(ConfigParam::EpochStabilityDepth(epoch_stability_depth))
        }
        for leader_id in consensus_leader_ids {
            initial_ents.push(ConfigParam::ConsensusLeaderId(leader_id.0))
        }
        if let Some(consensus_genesis_praos_param_d) = consensus_genesis_praos_param_d {
            initial_ents.push(ConfigParam::ConsensusGenesisPraosParamD(
                consensus_genesis_praos_param_d.0,
            ))
        }
        if let Some(consensus_genesis_praos_param_f) = consensus_genesis_praos_param_f {
            initial_ents.push(ConfigParam::ConsensusGenesisPraosActiveSlotsCoeff(
                consensus_genesis_praos_param_f.0,
            ))
        }
        initial_ents
    }
}

impl Update {
    fn to_message(self) -> Message {
        let proposal = UpdateProposal {
            max_number_of_transactions_per_block: self.max_number_of_transactions_per_block,
            bootstrap_key_slots_percentage: self.bootstrap_key_slots_percentage,
            consensus_version: None,
            bft_leaders: None,
            allow_account_creation: self.allow_account_creation,
            linear_fees: self.linear_fee.map(|linear_fee| LinearFee {
                constant: linear_fee.constant,
                coefficient: linear_fee.coefficient,
                certificate: linear_fee.certificate,
            }),
            slot_duration: None,
            epoch_stability_depth: None,
        };
        // FIXME: we probably want to sign using an actual BFT leader
        // here, but currently the update proposal in block 0 is not
        // verified anyway. So use a dummy proposer / signature.
        Message::UpdateProposal(SignedUpdateProposal {
            proposal: UpdateProposalWithProposer {
                proposal,
                proposer_id: PublicKey::<Ed25519Extended>::try_from_bech32_str(
                    "ed25519e_pk144xm656epf857f8ljfx4qwrc9xfshltfdhl87444sm3mzv78ru8sknr26a",
                )
                .unwrap()
                .into(),
            },
            signature: SignatureRaw(vec![]),
        })
    }
    fn from_proposal(update_proposal: &UpdateProposal) -> Self {
        Update {
            max_number_of_transactions_per_block: update_proposal
                .max_number_of_transactions_per_block,
            bootstrap_key_slots_percentage: update_proposal.bootstrap_key_slots_percentage,
            allow_account_creation: update_proposal.allow_account_creation,
            linear_fee: update_proposal
                .linear_fees
                .map(|linear_fee| InitialLinearFee {
                    constant: linear_fee.constant,
                    coefficient: linear_fee.coefficient,
                    certificate: linear_fee.certificate,
                }),
        }
    }
}

pub fn documented_example(now: std::time::SystemTime) -> String {
    let secs = now
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!(include_str!("DOCUMENTED_EXAMPLE.yaml"), now = secs)
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_yaml;

    #[test]
    fn conversion_to_and_from_message_preserves_data() {
        let genesis_yaml = r#"
---
blockchain_configuration:
  block0_date: 123456789
  discrimination: test
  block0_consensus: bft
  slots_per_epoch: 5
  slot_duration: 15
  epoch_stability_depth: 10
  consensus_leader_ids:
    - ed25519e_pk1hj8k4jyhsrva7ndynak25jagf3qcj4usnp54gnzvrejnwrufxpgqytzy6u
    - ed25519e_pk173x5f5xhg66x9yl4x50wnqg9mfwmmt4fma0styptcq4fuyvg3p7q9zxvy7
  consensus_genesis_praos_param_d: "0.222"
  consensus_genesis_praos_param_f: "0.444"
initial_setting:
  max_number_of_transactions_per_block: 255
  bootstrap_key_slots_percentage: 4
  allow_account_creation: true
  linear_fee:
    coefficient: 1
    constant: 2
    certificate: 4
initial_funds:
  - address: ta1svy0mwwm7mdwcuj308aapjw6ra4c3e6cygd0f333nvtjzxg8ahdvxlswdf0
    value: 10000
initial_certs:
  - cert1qsqqqqqqqqqqqqqqqqqqqqqqqr2sr5860cvq6vuc05tlwl9lwrs5vw7wq8fjm9fw6mucy0cdd84n0c6ljv2p03s8tc8nukzcrx87zkp3hflm2ymglghs4sn60xgryu57pznzff92ldaymh34u36z6fvdqnzl8my8ucttn09sehq8pdgrle35k3spqpq2s44c5mudrr2c3d0pelf75tjk4ezmsqfxhvjlawxvmcnluc0tcl7kfh4hveatrfuu5fzg74hxpucf0sh6v4l7hhkpneaa02lmp6j8q5jqgzt4
legacy_funds:
  - address: 48mDfYyQn21iyEPzCfkATEHTwZBcZJqXhRJezmswfvc6Ne89u1axXsiazmgd7SwT8VbafbVnCvyXhBSMhSkPiCezMkqHC4dmxRahRC86SknFu6JF6hwSg8
    value: 123
  - address: 48mDfYyQn21iyEPzCfkATEHTwZBcZJqXhRJezmswfvc6Ne89u1axXsiazmgd7SwT8VbafbVnCvyXhBSMhSkPiCezMkqHC4dmxRahRC86SknFu6JF6hwSg8
    value: 456
            "#.trim();
        let genesis: Genesis =
            serde_yaml::from_str(genesis_yaml).expect("Failed to deserialize YAML");

        let block = genesis.to_block();
        let new_genesis = Genesis::from_block(&block).expect("Failed to build genesis");

        let new_genesis_yaml =
            serde_yaml::to_string(&new_genesis).expect("Failed to serialize YAML");
        assert_eq!(
            genesis_yaml, new_genesis_yaml,
            "\nGenesis YAML has changed after conversions:\n{}\n",
            new_genesis_yaml
        );
    }
}
