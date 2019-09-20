pub mod error;
pub mod graphql;
mod indexing;
mod set;

use self::error::{Error, ErrorKind, Result};
use self::graphql::Context;
use self::indexing::{
    Addresses, Blocks, ChainLengths, EpochData, Epochs, ExplorerBlock, Transactions,
};
use self::set::HamtSet as Set;

use self::future::Either;
use crate::blockcfg::{
    Block, ChainLength, ConfigParam, ConfigParams, ConsensusVersion, Epoch, Fragment, FragmentId,
    HeaderHash,
};
use crate::blockchain::{Blockchain, Multiverse};
use crate::intercom::ExplorerMsg;
use crate::utils::task::{Input, TokioServiceInfo};
use chain_addr::Discrimination;
use chain_core::property::Block as _;
use chain_core::property::Fragment as _;
use chain_impl_mockchain::multiverse::GCRoot;
use imhamt;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::prelude::*;
use tokio::sync::lock::{Lock, LockGuard};

#[derive(Clone)]
pub struct Explorer {
    pub db: ExplorerDB,
    pub schema: Arc<graphql::Schema>,
}

#[derive(Clone)]
pub struct ExplorerDB {
    multiverse: Multiverse<State>,
    longest_chain_tip: Lock<Block>,
    blockchain_config: BlockchainConfig,
}

#[derive(Clone)]
struct BlockchainConfig {
    discrimination: Discrimination,
    consensus_version: ConsensusVersion,
}

#[derive(Clone)]
struct State {
    transactions: Transactions,
    blocks: Blocks,
    addresses: Addresses,
    epochs: Epochs,
    chain_lengths: ChainLengths,
}

#[derive(Clone)]
pub struct Settings {
    pub address_bech32_prefix: String,
}

impl Explorer {
    pub fn new(db: ExplorerDB, schema: graphql::Schema) -> Explorer {
        Explorer {
            db,
            schema: Arc::new(schema),
        }
    }

    pub fn context(&self) -> Context {
        Context {
            db: self.db.clone(),
            settings: Settings {
                // FIXME: Hardcoded value. This could come from the node settings.
                address_bech32_prefix: "ca".to_owned(),
            },
        }
    }

    pub fn handle_input(
        &mut self,
        info: &TokioServiceInfo,
        input: Input<ExplorerMsg>,
    ) -> impl Future<Item = (), Error = ()> {
        let _logger = info.logger();
        let bquery = match input {
            Input::Shutdown => {
                return future::ok(());
            }
            Input::Input(msg) => msg,
        };

        let mut explorer_db = self.db.clone();
        let logger = info.logger().clone();
        match bquery {
            ExplorerMsg::NewBlock(block) => info.spawn(explorer_db.apply_block(block).then(
                move |result| match result {
                    Ok(_gc_root) => Ok(()),
                    Err(err) => Err(error!(logger, "Explorer error: {}", err)),
                },
            )),
        }
        future::ok::<(), ()>(())
    }
}

impl ExplorerDB {
    pub fn bootstrap(block0: Block, blockchain: &Blockchain) -> Result<Self> {
        let blockchain_config = BlockchainConfig::from_config_params(
            block0
                .contents
                .iter()
                .filter_map(|fragment| match fragment {
                    Fragment::Initial(config_params) => Some(config_params),
                    _ => None,
                })
                .next()
                .expect("the Initial fragment to be present in the genesis block"),
        );

        let block = ExplorerBlock::resolve_from(
            &block0,
            blockchain_config.discrimination,
            &Transactions::new(),
            &Blocks::new(),
        );

        let blocks = apply_block_to_blocks(Blocks::new(), &block)?;
        let epochs = apply_block_to_epochs(Epochs::new(), &block)?;
        let chain_lengths = apply_block_to_chain_lengths(ChainLengths::new(), &block)?;
        let transactions = apply_block_to_transactions(Transactions::new(), &block)?;

        // TODO: Initialize this
        let addresses = Addresses::new();

        let initial_state = State {
            blocks,
            epochs,
            chain_lengths,
            transactions,
            addresses,
        };

        let multiverse = Multiverse::<State>::new();
        // This blocks the thread, but it's only on the node startup when the explorer
        // is enabled
        multiverse
            .insert(block0.chain_length(), block0.id(), initial_state)
            .wait()
            .expect("The multiverse to be empty");

        let block0_id = block0.id().clone();

        let bootstraped_db = ExplorerDB {
            multiverse,
            longest_chain_tip: Lock::new(block0),
            blockchain_config,
        };

        blockchain
            .storage()
            // FIXME: Unhardcode the "HEAD"
            .get_tag("HEAD".to_owned())
            .map_err(|err| err.into())
            .and_then(move |head_option| match head_option {
                None => Either::A(future::err(Error::from(ErrorKind::BootstrapError(
                    "Couldn't read the HEAD tag from storage".to_owned(),
                )))),
                Some(head) => Either::B(
                    blockchain
                        .storage()
                        .stream_from_to(block0_id, head)
                        .map_err(|err| Error::from(err)),
                ),
            })
            .and_then(move |stream_option| match stream_option {
                None => Either::A(future::err(Error::from(ErrorKind::BootstrapError(
                    "Couldn't iterate from Block0 to HEAD".to_owned(),
                )))),
                Some(stream) => Either::B(future::ok(stream)),
            })
            .and_then(move |stream| {
                stream
                    .map_err(|err| Error::from(err))
                    .fold(bootstraped_db, |mut db, block| {
                        db.apply_block(block).and_then(|_gc_root| Ok(db))
                    })
            })
            .wait()
    }

    pub fn apply_block(&mut self, block: Block) -> impl Future<Item = GCRoot, Error = Error> {
        let previous_block = block.header.block_parent_hash();
        let chain_length = block.header.chain_length();
        let block_id = block.header.hash();
        let multiverse = self.multiverse.clone();
        let block1 = block.clone();
        let block2 = block.clone();
        let discrimination = self.blockchain_config.discrimination.clone();

        multiverse
            .get(*previous_block)
            .map_err(|_: Infallible| unreachable!())
            .and_then(move |maybe_previous_state| {
                let block = block1;
                match maybe_previous_state {
                    Some(state) => {
                        let State {
                            transactions,
                            blocks,
                            addresses,
                            epochs,
                            chain_lengths,
                        } = state;

                        let explorer_block = ExplorerBlock::resolve_from(
                            &block,
                            discrimination,
                            &transactions,
                            &blocks,
                        );

                        Ok((
                            apply_block_to_transactions(transactions, &explorer_block)?,
                            apply_block_to_blocks(blocks, &explorer_block)?,
                            apply_block_to_addresses(addresses, &explorer_block)?,
                            apply_block_to_epochs(epochs, &explorer_block)?,
                            apply_block_to_chain_lengths(chain_lengths, &explorer_block)?,
                        ))
                    }
                    None => Err(Error::from(ErrorKind::AncestorNotFound(format!(
                        "{}",
                        block.id()
                    )))),
                }
            })
            .and_then(
                move |(transactions, blocks, addresses, epochs, chain_lengths)| {
                    let chain_length = chain_length.clone();
                    let block_id = block_id.clone();
                    multiverse
                        .insert(
                            chain_length,
                            block_id,
                            State {
                                transactions,
                                blocks,
                                addresses,
                                epochs,
                                chain_lengths,
                            },
                        )
                        .map_err(|_: Infallible| unreachable!())
                },
            )
            .join(
                self.update_longest_chain_tip(block2)
                    .map_err(|_: Infallible| unreachable!()),
            )
            .and_then(|(gc_root, _)| Ok(gc_root))
    }

    fn update_longest_chain_tip(
        &mut self,
        new_block: Block,
    ) -> impl Future<Item = (), Error = Infallible> {
        get_lock(&self.longest_chain_tip).and_then(|mut current| {
            if new_block.header.chain_length() > current.header.chain_length() {
                *current = new_block;
            }
            Ok(())
        })
    }

    pub fn get_block(
        &self,
        block_id: &HeaderHash,
    ) -> impl Future<Item = Option<ExplorerBlock>, Error = Infallible> {
        let multiverse = self.multiverse.clone();
        let block_id = block_id.clone();
        get_lock(&self.longest_chain_tip).and_then(move |tip| {
            multiverse.get((*tip).id()).and_then(move |maybe_state| {
                let state = maybe_state.expect("the longest chain to be indexed");
                Ok(state.blocks.lookup(&block_id).map(|b| (*b).clone()))
            })
        })
    }

    pub fn get_epoch(
        &self,
        epoch: Epoch,
    ) -> impl Future<Item = Option<EpochData>, Error = Infallible> {
        let multiverse = self.multiverse.clone();
        let epoch = epoch.clone();
        get_lock(&self.longest_chain_tip).and_then(move |tip| {
            multiverse.get((*tip).id()).and_then(move |maybe_state| {
                let state = maybe_state.expect("the longest chain to be indexed");
                Ok(state.epochs.lookup(&epoch).map(|e| (*e).clone()))
            })
        })
    }

    pub fn find_block_by_chain_length(
        &self,
        chain_length: ChainLength,
    ) -> impl Future<Item = Option<HeaderHash>, Error = Infallible> {
        let multiverse = self.multiverse.clone();
        get_lock(&self.longest_chain_tip).and_then(move |tip| {
            multiverse.get((*tip).id()).and_then(move |maybe_state| {
                let state = maybe_state.expect("the longest chain to be indexed");
                Ok(state
                    .chain_lengths
                    .lookup(&chain_length)
                    .map(|b| (*b).clone()))
            })
        })
    }

    pub fn find_block_by_transaction(
        &self,
        transaction: &FragmentId,
    ) -> impl Future<Item = Option<HeaderHash>, Error = Infallible> {
        let multiverse = self.multiverse.clone();
        let transaction = transaction.clone();
        get_lock(&self.longest_chain_tip).and_then(move |tip| {
            multiverse.get((*tip).id()).and_then(move |maybe_state| {
                let state = maybe_state.expect("the longest chain to be indexed");
                Ok(state.transactions.lookup(&transaction).map(|id| id.clone()))
            })
        })
    }
}

fn get_lock<L>(lock: &Lock<L>) -> impl Future<Item = LockGuard<L>, Error = Infallible> {
    let mut lock = (*lock).clone();
    future::poll_fn(move || Ok(lock.poll_lock()))
}

fn apply_block_to_transactions(
    transactions: Transactions,
    block: &ExplorerBlock,
) -> Result<Transactions> {
    let block_id = block.id();
    let ids = block.transactions.values().map(|tx| tx.id());

    let mut transactions = transactions;
    for id in ids {
        transactions = transactions
            .insert(id, block_id)
            .map_err(|_| ErrorKind::TransactionAlreadyExists(format!("{}", id)))?;
    }

    Ok(transactions)
}

fn apply_block_to_blocks(blocks: Blocks, block: &ExplorerBlock) -> Result<Blocks> {
    let block_id = block.id();
    blocks
        .insert(block_id, (*block).clone())
        .map_err(|_| Error::from(ErrorKind::BlockAlreadyExists(format!("{}", block_id))))
}

fn apply_block_to_addresses(addresses: Addresses, block: &ExplorerBlock) -> Result<Addresses> {
    let mut addresses = addresses;
    let transactions = block.transactions.values();

    for tx in transactions {
        let id = tx.id();
        for output in tx.outputs() {
            addresses = addresses
                .insert_or_update(
                    output.address.clone(),
                    Set::new().add_element(id.clone()),
                    |set| {
                        let new_set = set.add_element(id.clone());
                        Ok::<Option<Set<FragmentId>>, Infallible>(Some(new_set))
                    },
                )
                .expect("insert or update to always work")
        }

        for input in tx.inputs() {
            addresses = addresses
                .insert_or_update(
                    input.address.clone(),
                    Set::new().add_element(id.clone()),
                    |set| {
                        let new_set = set.add_element(id.clone());
                        Ok::<Option<Set<FragmentId>>, Infallible>(Some(new_set))
                    },
                )
                .expect("insert or update to always work")
        }
    }

    Ok(addresses)
}

fn apply_block_to_epochs(epochs: Epochs, block: &ExplorerBlock) -> Result<Epochs> {
    let epoch_id = block.date().epoch;
    let block_id = block.id();

    epochs
        .insert_or_update(
            epoch_id,
            EpochData {
                first_block: block_id,
                last_block: block_id,
                total_blocks: 0,
            },
            |data| {
                Ok(Some(EpochData {
                    last_block: block_id,
                    total_blocks: data.total_blocks + 1,
                    ..*data
                }))
            },
        )
        .map_err(|_: imhamt::InsertOrUpdateError<Infallible>| {
            unreachable!();
        })
}

fn apply_block_to_chain_lengths(
    chain_lengths: ChainLengths,
    block: &ExplorerBlock,
) -> Result<ChainLengths> {
    let new_block_chain_length = block.chain_length();
    let new_block_hash = block.id();
    chain_lengths
        .insert(new_block_chain_length, new_block_hash)
        .map_err(|_| {
            // I think this shouldn't happen
            Error::from(ErrorKind::ChainLengthBlockAlreadyExists(u32::from(
                new_block_chain_length,
            )))
        })
}

impl BlockchainConfig {
    fn from_config_params(params: &ConfigParams) -> BlockchainConfig {
        let discrimination = params
            .iter()
            .filter_map(|param| match param {
                ConfigParam::Discrimination(discrimination) => Some(discrimination.clone()),
                _ => None,
            })
            .next()
            .expect("the discrimination to be present");

        let consensus_version = params
            .iter()
            .filter_map(|param| match param {
                ConfigParam::ConsensusVersion(version) => Some(version.clone()),
                _ => None,
            })
            .next()
            .expect("consensus version to be present");

        BlockchainConfig {
            discrimination,
            consensus_version,
        }
    }
}
