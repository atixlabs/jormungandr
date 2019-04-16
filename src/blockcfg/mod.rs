use chain_core::property::HasMessages as _;
pub use network_core::gossip::Gossip;

pub use chain_impl_mockchain::{
    block::{Block, BlockBuilder, BlockDate, ChainLength, Epoch, Header, HeaderHash, SlotId},
    config::{self, Block0Date, ConfigParam},
    leadership::{BftLeader, GenesisLeader, Leader, LeaderOutput, Leadership},
    ledger::{Ledger, LedgerParameters, LedgerStaticParameters},
    message::{InitialEnts, Message, MessageId},
    multiverse::Multiverse,
};
use std::time::{Duration, SystemTime};

custom_error! {pub Block0Malformed
    NoInitialSettings = "Missing its initial settings",
    NoStartTime = "Missing `block0-start' value in the block0",
    NoDiscrimination = "Missing `discrimination' value in the block0",
    NoSlotDuration = "Missing `slot_duration' value in the block0",
}
custom_error! {pub Block0Error
    CannotParseEntity{source: config::Error} = "Block0 Initial settings",
    Malformed{source: Block0Malformed} = "Block0 is invalid or malformed"
}

fn block_0_get_initial(block: &Block) -> Result<&InitialEnts, Block0Error> {
    for message in block.messages() {
        if let Message::Initial(init) = message {
            return Ok(init);
        }
    }

    Err(Block0Malformed::NoInitialSettings.into())
}

pub fn block_0_get_slot_duration(block: &Block) -> Result<std::time::Duration, Block0Error> {
    let mut duration = None;
    for message in block.messages() {
        if let Message::Update(proposal) = message {
            duration = proposal.slot_duration;
        }
    }

    if let Some(duration) = duration {
        Ok(std::time::Duration::from_secs(duration as u64))
    } else {
        Err(Block0Malformed::NoSlotDuration.into())
    }
}

pub fn block_0_get_slots_per_epoch(block: &Block) -> Result<Option<usize>, Block0Error> {
    Ok(block
        .messages()
        .find_map(|message| match message {
            Message::Initial(ents) => Some(ents),
            _ => None,
        })
        .ok_or(Block0Malformed::NoInitialSettings)?
        .iter()
        .find_map(|message| match message {
            ConfigParam::SlotsPerEpoch(slots) => Some(*slots as usize),
            _ => None,
        }))
}

pub fn block_0_get_start_time(block: &Block) -> Result<std::time::SystemTime, Block0Error> {
    let ents = block_0_get_initial(block)?;

    for config in ents.iter() {
        if let ConfigParam::Block0Date(date) = config {
            return Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(date.0));
        }
    }
    Err(Block0Malformed::NoStartTime.into())
}
