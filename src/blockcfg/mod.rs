//! This module provides the different abstractions for the different
//! part of the blockchain.
//!
//! It has been split into 3 components:
//!
//! 1. chain: all the components that chains blocks together;
//! 2. ledger: the transaction model of a blockchain;
//! 3. consensus: the consensus model of the blockchain.
//!

pub use chain_core::property::{
    Block, BlockDate, BlockId, Deserialize, FromStr, HasHeader, HasTransaction, Header,
    LeaderSelection, Ledger, Serialize, Settings, Transaction, TransactionId, Update,
};
pub use network_core::gossip::Gossip;

pub mod genesis_data;
pub mod mock;

use std::fmt::{Debug, Display};

pub trait BlockConfig {
    type Block: Block<Id = Self::BlockHash, Date = Self::BlockDate>
        + HasTransaction<Transaction = Self::Transaction>
        + HasHeader<Header = Self::BlockHeader>
        + Send
        + Sync;
    type BlockDate: BlockDate + Display + FromStr;
    type BlockHash: BlockId + Display + Send;
    type BlockHeader: Header<Id = Self::BlockHash, Date = Self::BlockDate>
        + Clone
        + Send
        + Sync
        + Debug;
    type Transaction: Transaction<Id = Self::TransactionId> + Serialize + Send + Clone;
    type TransactionId: TransactionId + Serialize + Deserialize + Send;
    type GenesisData;

    type Ledger: Ledger<Transaction = Self::Transaction>;
    type Settings: Settings<Block = Self::Block>;
    type Leader: LeaderSelection<Block = Self::Block>;
    type Update: Update;
    type Gossip: Gossip + Clone + Send + Sync + Debug;

    type NodeSigningKey;

    fn make_block(
        secret_key: &Self::NodeSigningKey,
        settings: &Self::Settings,
        ledger: &Self::Ledger,
        block_date: <Self::Block as Block>::Date,
        transactions: Vec<Self::Transaction>,
    ) -> Self::Block;
}
