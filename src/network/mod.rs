//! all the network related actions and processes
//!
//! This module only provides and handle the different connections
//! and act as message passing between the other modules (blockchain,
//! transactions...);
//!

mod connections;
mod grpc;
// TODO: to be ported
//mod ntt;
pub mod p2p_topology;
mod service;

use self::{
    connections::Connections,
    p2p_topology::{self as p2p, P2pTopology},
};
use crate::blockcfg::BlockConfig;
use crate::blockchain::BlockchainR;
use crate::intercom::{BlockMsg, ClientMsg, TransactionMsg};
use crate::settings::start::network::{Configuration, Listen, Peer, Protocol};
use crate::utils::task::TaskMessageBox;

use chain_core::property;
use futures::prelude::*;
use futures::{
    future,
    stream::{self, Stream},
};

use std::{net::SocketAddr, sync::Arc, time::Duration};

type Connection = SocketAddr;

pub trait NetworkBlockConfig:
    BlockConfig
    + network_grpc::client::ProtocolConfig<
        Block = <Self as BlockConfig>::Block,
        Header = <Self as BlockConfig>::BlockHeader,
        BlockId = <Self as BlockConfig>::BlockHash,
        BlockDate = <Self as BlockConfig>::BlockDate,
        Gossip = <Self as BlockConfig>::Gossip,
    >
{
}

impl<B> NetworkBlockConfig for B where
    B: BlockConfig
        + network_grpc::client::ProtocolConfig<
            Block = <Self as BlockConfig>::Block,
            Header = <Self as BlockConfig>::BlockHeader,
            BlockId = <Self as BlockConfig>::BlockHash,
            BlockDate = <Self as BlockConfig>::BlockDate,
            Gossip = <Self as BlockConfig>::Gossip,
        >
{
}

/// all the different channels the network may need to talk to
pub struct Channels<B: BlockConfig> {
    pub client_box: TaskMessageBox<ClientMsg<B>>,
    pub transaction_box: TaskMessageBox<TransactionMsg<B>>,
    pub block_box: TaskMessageBox<BlockMsg<B>>,
}

impl<B: BlockConfig> Clone for Channels<B> {
    fn clone(&self) -> Self {
        Channels {
            client_box: self.client_box.clone(),
            transaction_box: self.transaction_box.clone(),
            block_box: self.block_box.clone(),
        }
    }
}

pub struct GlobalState<B: NetworkBlockConfig> {
    pub config: Arc<Configuration>,
    pub channels: Channels<B>,
    pub topology: P2pTopology,
    pub node: p2p::Node,
    pub connections: Connections<B>,
}

impl<B: NetworkBlockConfig> GlobalState<B> {
    /// the network global state
    pub fn new(config: &Configuration, channels: Channels<B>) -> Self {
        let node_id = p2p_topology::Id::generate(&mut rand::thread_rng());
        let node_address = config
            .public_address
            .clone()
            .expect("only support the full nodes for now")
            .0
            .into();
        let mut node = p2p_topology::Node::new(node_id, node_address);

        // TODO: load the subscriptions from the config
        p2p_topology::add_transaction_subscription(&mut node, p2p_topology::InterestLevel::High);
        p2p_topology::add_block_subscription(&mut node, p2p_topology::InterestLevel::High);

        let p2p_topology = P2pTopology::new(node.clone());

        let arc_config = Arc::new(config.clone());
        GlobalState {
            config: arc_config,
            channels: channels,
            topology: p2p_topology,
            node,
            connections: Default::default(),
        }
    }
}

impl<B: NetworkBlockConfig> Clone for GlobalState<B> {
    fn clone(&self) -> Self {
        GlobalState {
            config: self.config.clone(),
            channels: self.channels.clone(),
            topology: self.topology.clone(),
            node: self.node.clone(),
            connections: self.connections.clone(),
        }
    }
}

pub struct ConnectionState<B: BlockConfig> {
    /// The global network configuration
    pub global_network_configuration: Arc<Configuration>,

    /// the channels the connection will need to have to
    /// send messages too
    pub channels: Channels<B>,

    /// the timeout to wait for unbefore the connection replies
    pub timeout: Duration,

    /// the local (to the task) connection details
    pub connection: Connection,

    /// Network topology reference.
    pub topology: P2pTopology,

    /// Node inside network topology.
    pub node: p2p::Node,
}

impl<B: BlockConfig> Clone for ConnectionState<B> {
    fn clone(&self) -> Self {
        ConnectionState {
            global_network_configuration: self.global_network_configuration.clone(),
            channels: self.channels.clone(),
            timeout: self.timeout,
            connection: self.connection.clone(),
            node: self.node.clone(),
            topology: self.topology.clone(),
        }
    }
}

impl<B: NetworkBlockConfig> ConnectionState<B> {
    fn new_listen(global: &GlobalState<B>, listen: &Listen) -> Self {
        ConnectionState {
            global_network_configuration: global.config.clone(),
            channels: global.channels.clone(),
            timeout: listen.timeout,
            connection: listen.connection,
            node: global.node.clone(),
            topology: global.topology.clone(),
        }
    }

    fn new_peer(global: &GlobalState<B>, peer: &Peer) -> Self {
        ConnectionState {
            global_network_configuration: global.config.clone(),
            channels: global.channels.clone(),
            timeout: peer.timeout,
            connection: peer.connection,
            node: global.node.clone(),
            topology: global.topology.clone(),
        }
    }
}

pub fn run<B>(config: Configuration, channels: Channels<B>)
where
    B: NetworkBlockConfig + 'static,
{
    // TODO: the node needs to be saved/loaded
    //
    // * the ID needs to be consistent between restart;
    let state = GlobalState::new(&config, channels);
    let protocol = config.protocol;

    let state_listener = state.clone();
    // open the port for listening/accepting other peers to connect too
    let listener = if let Some(public_address) = config
        .public_address
        .and_then(move |addr| addr.to_socketaddr())
    {
        let protocol = protocol.clone();
        match protocol.clone() {
            Protocol::Grpc => {
                let listen = Listen::new(public_address, protocol);
                grpc::run_listen_socket(public_address, listen, state_listener)
            }
            Protocol::Ntt => unimplemented!(),
        }
    } else {
        unimplemented!()
    };

    let state_connection = state.clone();
    let addrs = config
        .trusted_addresses
        .iter()
        .filter_map(|paddr| paddr.to_socketaddr())
        .collect::<Vec<_>>();
    let connections = stream::iter_ok(addrs).for_each(move |addr| {
        let peer = Peer::new(addr, Protocol::Grpc);
        let connections = state_connection.connections;
        grpc::run_connect_socket(peer, state_connection.clone()).and_then(|client| {
            connections.add_connection(addr, client);
            future::ok(())
        })
    });

    tokio::run(connections.join(listener).map(|_| ()));
}

pub fn bootstrap<B>(config: &Configuration, blockchain: BlockchainR<B>)
where
    B: NetworkBlockConfig,
    <B::Ledger as property::Ledger>::Update: Clone,
    <B::Settings as property::Settings>::Update: Clone,
    <B::Leader as property::LeaderSelection>::Update: Clone,
{
    if config.protocol != Protocol::Grpc {
        unimplemented!()
    }
    let peer = config.trusted_addresses.iter().next();
    match peer.and_then(|a| a.to_socketaddr()) {
        Some(address) => {
            let peer = Peer::new(address, Protocol::Grpc);
            grpc::bootstrap_from_peer(peer, blockchain)
        }
        None => {
            warn!("no gRPC peers specified, skipping bootstrap");
        }
    }
}
