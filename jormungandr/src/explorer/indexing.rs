use super::set::HamtSet as Set;
use imhamt;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;

use crate::blockcfg::{Block, BlockDate, ChainLength, Epoch, Fragment, FragmentId, HeaderHash};
use chain_addr::{Address, Discrimination};
use chain_core::property::Block as _;
use chain_core::property::Fragment as _;
use chain_impl_mockchain::transaction::{AuthenticatedTransaction, InputEnum, Witness};
use chain_impl_mockchain::value::Value;

pub type Hamt<K, V> = imhamt::Hamt<DefaultHasher, K, V>;

pub type Transactions = Hamt<FragmentId, HeaderHash>;
pub type Blocks = Hamt<HeaderHash, ExplorerBlock>;
pub type ChainLengths = Hamt<ChainLength, HeaderHash>;

pub type Addresses = Hamt<Address, Set<FragmentId>>;
pub type Epochs = Hamt<Epoch, EpochData>;

#[derive(Clone)]
pub struct ExplorerBlock {
    pub transactions: HashMap<FragmentId, ExplorerTransaction>,
    pub id: HeaderHash,
    pub date: BlockDate,
    pub chain_length: ChainLength,
    pub parent_hash: HeaderHash,
}

#[derive(Clone)]
pub struct ExplorerTransaction {
    id: FragmentId,
    inputs: Vec<ExplorerInput>,
    outputs: Vec<ExplorerOutput>,
}

#[derive(Clone)]
pub struct ExplorerInput {
    pub address: Address,
    pub value: Value,
}

#[derive(Clone)]
pub struct ExplorerOutput {
    pub address: Address,
    pub value: Value,
}

#[derive(Clone)]
pub struct EpochData {
    pub first_block: HeaderHash,
    pub last_block: HeaderHash,
    pub total_blocks: u32,
}

impl ExplorerBlock {
    pub fn resolve_from(
        block: &Block,
        discrimination: Discrimination,
        prev_transactions: &Transactions,
        prev_blocks: &Blocks,
    ) -> ExplorerBlock {
        let fragments = block.contents.iter();
        let id = block.id();
        let chain_length = block.chain_length();

        let transactions = fragments
            .filter_map(|fragment| {
                let fragment_id = fragment.id();
                match fragment {
                    Fragment::Transaction(auth_tx) => Some((
                        fragment_id,
                        ExplorerTransaction::from(
                            &fragment_id,
                            auth_tx,
                            discrimination,
                            prev_transactions,
                            prev_blocks,
                        ),
                    )),
                    Fragment::OwnerStakeDelegation(auth_tx) => Some((
                        fragment_id,
                        ExplorerTransaction::from(
                            &fragment_id,
                            auth_tx,
                            discrimination,
                            prev_transactions,
                            prev_blocks,
                        ),
                    )),
                    Fragment::StakeDelegation(auth_tx) => Some((
                        fragment_id,
                        ExplorerTransaction::from(
                            &fragment_id,
                            auth_tx,
                            discrimination,
                            prev_transactions,
                            prev_blocks,
                        ),
                    )),
                    Fragment::PoolRegistration(auth_tx) => Some((
                        fragment_id,
                        ExplorerTransaction::from(
                            &fragment_id,
                            auth_tx,
                            discrimination,
                            prev_transactions,
                            prev_blocks,
                        ),
                    )),
                    Fragment::PoolManagement(auth_tx) => Some((
                        fragment_id,
                        ExplorerTransaction::from(
                            &fragment_id,
                            auth_tx,
                            discrimination,
                            prev_transactions,
                            prev_blocks,
                        ),
                    )),
                    _ => None,
                }
            })
            .collect();

        ExplorerBlock {
            id,
            transactions,
            chain_length,
            date: *block.header.block_date(),
            parent_hash: block.parent_id(),
        }
    }

    pub fn id(&self) -> HeaderHash {
        self.id
    }

    pub fn date(&self) -> BlockDate {
        self.date
    }

    pub fn chain_length(&self) -> ChainLength {
        self.chain_length
    }
}

impl ExplorerTransaction {
    pub fn from<T>(
        id: &FragmentId,
        auth_tx: &AuthenticatedTransaction<Address, T>,
        discrimination: Discrimination,
        transactions: &Transactions,
        blocks: &Blocks,
    ) -> ExplorerTransaction {
        let outputs = auth_tx.transaction.outputs.iter();
        let inputs = auth_tx.transaction.inputs.iter();
        let witnesses = auth_tx.witnesses.iter();

        let new_outputs = outputs
            .map(|output| ExplorerOutput {
                address: output.address.clone(),
                value: output.value,
            })
            .collect();

        let new_inputs = inputs
            .map(|i| i.to_enum())
            .zip(witnesses)
            .filter_map(|input_with_witness| match input_with_witness {
                (InputEnum::AccountInput(id, value), Witness::Account(_)) => {
                    let kind = chain_addr::Kind::Account(
                        id.to_single_account()
                            .expect("the input to be validated")
                            .into(),
                    );
                    let address = Address(discrimination, kind);
                    Some(ExplorerInput { address, value })
                }
                (InputEnum::AccountInput(_id, _value), Witness::Multisig(_)) => {
                    // TODO
                    None
                }
                (InputEnum::UtxoInput(utxo_pointer), _witness) => {
                    let tx = utxo_pointer.transaction_id;
                    let index = utxo_pointer.output_index;

                    let block_id = transactions.lookup(&tx).expect("the input to be validated");

                    let block = blocks.lookup(&block_id).expect("the input to be validated");

                    let output = &block.transactions[&tx].outputs[index as usize];

                    Some(ExplorerInput {
                        address: output.address.clone(),
                        value: output.value,
                    })
                }
                _ => None,
            })
            .collect();

        ExplorerTransaction {
            id: *id,
            inputs: new_inputs,
            outputs: new_outputs,
        }
    }

    pub fn id(&self) -> FragmentId {
        self.id
    }

    pub fn inputs(&self) -> &Vec<ExplorerInput> {
        &self.inputs
    }

    pub fn outputs(&self) -> &Vec<ExplorerOutput> {
        &self.outputs
    }
}
