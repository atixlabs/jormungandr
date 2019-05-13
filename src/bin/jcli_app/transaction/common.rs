use chain_impl_mockchain::fee::LinearFee;
use jcli_app::transaction::{staging::Staging, Error};
use jcli_app::utils::io;
use std::{
    io::BufRead,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct CommonFees {
    #[structopt(long = "fee-constant", default_value = "0")]
    pub constant: u64,
    #[structopt(long = "fee-coefficient", default_value = "0")]
    pub coefficient: u64,
    #[structopt(long = "fee-certificate", default_value = "0")]
    pub certificate: u64,
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct CommonTransaction {
    /// place where the transaction is going to be save during its staging phase
    /// If a file is given, the transaction will be read from this file and
    /// modification will be written into this same file.
    /// If no file is given, the transaction will be read from the standard
    /// input and will be rendered in the standard output
    #[structopt(long = "staging", alias = "transaction")]
    pub staging_file: Option<PathBuf>,
}

impl CommonFees {
    pub fn linear_fee(&self) -> LinearFee {
        LinearFee::new(self.constant, self.coefficient, self.certificate)
    }
}

impl CommonTransaction {
    pub fn load(&self) -> Result<Staging, Error> {
        Staging::load(&self.staging_file)
    }

    pub fn store(&self, staging: &Staging) -> Result<(), Error> {
        staging.store(&self.staging_file)
    }
}

pub fn path_to_path_buf<P: AsRef<Path>>(path: &Option<P>) -> PathBuf {
    path.as_ref()
        .map(|path| path.as_ref().to_path_buf())
        .unwrap_or_default()
}

pub fn read_line<P: AsRef<Path>>(path: &Option<P>) -> Result<String, std::io::Error> {
    let mut line = String::new();
    io::open_file_read(path)?.read_line(&mut line)?;
    Ok(line)
}
