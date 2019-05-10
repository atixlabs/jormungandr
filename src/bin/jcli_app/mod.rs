mod address;
mod auto_completion;
mod block;
mod certificate;
mod debug;
mod key;
mod rest;
mod transaction;
mod utils;

use std::error::Error;
use structopt::StructOpt;

/// Jormungandr CLI toolkit
#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum JCli {
    /// Key Generation
    Key(key::Key),
    /// Address tooling and helper
    Address(address::Address),
    /// Block tooling and helper
    Genesis(block::Genesis),
    /// Send request to node REST API
    Rest(rest::Rest),
    /// Build and view offline transaction
    Transaction(transaction::Transaction),
    /// Debug tools for developers
    Debug(debug::Debug),
    /// Certificate generation tool
    Certificate(certificate::Certificate),
    /// Auto completion
    AutoCompletion(auto_completion::AutoCompletion),
}

impl JCli {
    pub fn exec(self) -> Result<(), Box<Error>> {
        match self {
            JCli::Key(key) => key.exec()?,
            JCli::Address(address) => address.exec(),
            JCli::Genesis(genesis) => genesis.exec(),
            JCli::Rest(rest) => rest.exec(),
            JCli::Transaction(transaction) => transaction.exec()?,
            JCli::Debug(debug) => debug.exec()?,
            JCli::Certificate(certificate) => certificate.exec()?,
            JCli::AutoCompletion(auto_completion) => auto_completion.exec::<Self>(),
        };
        Ok(())
    }
}
