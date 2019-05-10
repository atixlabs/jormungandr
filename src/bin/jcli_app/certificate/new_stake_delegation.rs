use chain_crypto::{Ed25519Extended, PublicKey};
use chain_impl_mockchain::certificate::{
    Certificate, CertificateContent, StakeDelegation as Delegation,
};
use jcli_app::certificate::{self, Error};
use jcli_app::utils::key_parser::parse_pub_key;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct StakeDelegation {
    /// the stake pool id
    #[structopt(name = "STAKE_POOL_ID", parse(try_from_str))]
    pub pool_id: chain_crypto::Blake2b256,
    /// the delegation key
    #[structopt(name = "DELEGATION_ID", parse(try_from_str = "parse_pub_key"))]
    pub stake_id: PublicKey<Ed25519Extended>,
    /// print the output signed certificate in the given file, if no file given
    /// the output will be printed in the standard output
    pub output: Option<PathBuf>,
}

impl StakeDelegation {
    pub fn exec(self) -> Result<(), Error> {
        let content = Delegation {
            stake_key_id: self.stake_id.into(),
            pool_id: self.pool_id.into(),
        };
        let cert = Certificate {
            content: CertificateContent::StakeDelegation(content),
            signatures: vec![],
        };
        certificate::write_cert(self.output, cert)
    }
}
