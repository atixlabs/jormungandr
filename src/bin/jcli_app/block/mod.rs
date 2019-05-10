extern crate chain_addr;
extern crate chain_core;
extern crate chain_impl_mockchain;
extern crate structopt;

use chain_core::property::{Block as _, Deserialize, Serialize};
use chain_impl_mockchain::block::Block;
use jcli_app::utils::{error::CustomErrorFiller, io};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use structopt::StructOpt;

mod yaml;

custom_error! {pub Error
    InputInvalid { source: std::io::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Invalid input file path '{}'", path.display()) }},
    OutputInvalid { source: std::io::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Invalid output file path '{}'", path.display()) }},
    BlockFileCorrupted { source: std::io::Error, filler: CustomErrorFiller } = "Block file corrupted",
    GenesisFileCorrupted { source: serde_yaml::Error, filler: CustomErrorFiller } = "Genesis file corrupted",
    BlockSerializationFailed { source: std::io::Error, filler: CustomErrorFiller } = "Failed to serialize block",
    GenesisSerializationFailed { source: serde_yaml::Error, filler: CustomErrorFiller } = "Failed to serialize genesis",
    BuildingGenesisFromBlock0Failed { source: self::yaml::Error } = "Failed to build genesis from block 0",
}

impl Genesis {
    pub fn exec(self) -> Result<(), Error> {
        match self {
            Genesis::Init => init_genesis_yaml(),
            Genesis::Encode(create_arguments) => encode_block_0(create_arguments),
            Genesis::Decode(info_arguments) => decode_block_0(info_arguments),
            Genesis::Hash(hash_arguments) => print_hash(hash_arguments),
        }
    }
}

fn init_genesis_yaml() -> Result<(), Error> {
    println!("{}", yaml::documented_example(std::time::SystemTime::now()));
    Ok(())
}

fn encode_block_0(common: Common) -> Result<(), Error> {
    let reader = common.input.open()?;
    let genesis: yaml::Genesis =
        serde_yaml::from_reader(reader).map_err(|source| Error::GenesisFileCorrupted {
            source,
            filler: CustomErrorFiller,
        })?;
    let block = genesis.to_block();
    block
        .serialize(common.open_output()?)
        .map_err(|source| Error::BlockSerializationFailed {
            source,
            filler: CustomErrorFiller,
        })
}

fn decode_block_0(common: Common) -> Result<(), Error> {
    let block = common.input.load_block()?;
    let yaml = yaml::Genesis::from_block(&block)?;
    serde_yaml::to_writer(common.open_output()?, &yaml).map_err(|source| {
        Error::GenesisSerializationFailed {
            source,
            filler: CustomErrorFiller,
        }
    })
}

fn print_hash(input: Input) -> Result<(), Error> {
    let block = input.load_block()?;
    println!("{}", block.id());
    Ok(())
}

/// create block 0 of the blockchain (i.e. the genesis block)
#[derive(StructOpt)]
#[structopt(name = "genesis", rename_all = "kebab-case")]
pub enum Genesis {
    /// Create a default Genesis file with appropriate documentation
    /// to help creating the YAML file
    Init,

    /// create the block 0 file (the genesis block of the blockchain)
    /// from a given yaml file
    ///
    Encode(Common),

    /// Decode the block 0 and print the corresponding YAML file
    Decode(Common),

    /// print the block hash (aka the block id) of the block 0
    Hash(Input),
}

#[derive(StructOpt)]
pub struct Input {
    /// the file path to the genesis file defining the block 0
    ///
    /// If not available the command will expect to read the configuration from
    /// the standard input.
    #[structopt(long = "input", parse(from_os_str), name = "FILE_INPUT")]
    input_file: Option<std::path::PathBuf>,
}

impl Input {
    fn open(&self) -> Result<impl BufRead, Error> {
        io::open_file_read(&self.input_file).map_err(|source| Error::InputInvalid {
            source,
            path: self.input_file.clone().unwrap_or_default(),
        })
    }

    fn load_block(&self) -> Result<Block, Error> {
        let reader = self.open()?;
        Block::deserialize(reader).map_err(|source| Error::BlockFileCorrupted {
            source,
            filler: CustomErrorFiller,
        })
    }
}

#[derive(StructOpt)]
pub struct Common {
    #[structopt(flatten)]
    input: Input,

    /// the file path to the block to create
    ///
    /// If not available the command will expect to write the block to
    /// to the standard output
    #[structopt(long = "output", parse(from_os_str), name = "FILE_OUTPUT")]
    output_file: Option<std::path::PathBuf>,
}

impl Common {
    fn open_output(&self) -> Result<impl Write, Error> {
        io::open_file_write(&self.output_file).map_err(|source| Error::OutputInvalid {
            source,
            path: self.output_file.clone().unwrap_or_default(),
        })
    }
}
