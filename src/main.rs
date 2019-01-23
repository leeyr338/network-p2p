
pub mod config;
pub mod p2p_protocol;
pub mod node_manager;
pub mod network;
pub mod citaprotocol;

use env_logger;
use log::{debug};
use futures::{
    prelude::*,
};
use std::{
    thread,
};
use std::sync::mpsc::channel;
use pubsub::start_pubsub;
use libproto::routing_key;
use libproto::router::{MsgType, SubModules, RoutingKey};
use libproto::{ Message, Response, TryInto, TryFrom };

use p2p::{
    builder::ServiceBuilder,
    SecioKeyPair,
};

use crate::config::NetConfig;

use crate::p2p_protocol::{
    SHandle,
    node_discovery::{
        DiscoveryProtocolMeta, NodesAddressManager,
    },
    transfer::{
        TransferProtocolMeta,
    }
};

use crate::node_manager::{
    NodesManager, DEFAULT_PORT, BroadcastReq,
};

use crate::network::{
    Network,
};

//include!(concat!(env!("OUT_DIR"), "./build_info.rs"));

fn main() {
    env_logger::init();

    // >>>> Init config
    // FIXME: Use App:new to get config
    let config_path = std::env::args().nth(1)
        .unwrap_or("/Volumes/x/cryptape/gettingStart/rust/network-p2p/examples/config/1_node.toml".to_string());
    debug!("config path {:?}", config_path);
    let config = NetConfig::new(&config_path);
    debug!("network config is {:?}", config);
    // <<<< End init config

    // >>>> Init pubsub
    // New transactions use a special channel, all new transactions come from:
    // JsonRpc -> Auth -> Network,
    // So the channel subscribe 'Auth' Request from MQ
    let (ctx_sub_auth, crx_sub_auth) = channel();
    let (ctx_pub_auth, crx_pub_auth) = channel();
    start_pubsub(
        "network_auth",
        routing_key!([Auth >> Request, Auth >> GetBlockTxn, Auth >> BlockTxn]),
        ctx_sub_auth,
        crx_pub_auth,
    );
    // <<<< End init pubsub

    // >>>> Init p2p protocols
    let mut nodes_mgr = NodesManager::from_config(config.clone());
    let mut network_mgr = Network::new();

    let discovery_meta = DiscoveryProtocolMeta::new(
        0,
        NodesAddressManager::new(nodes_mgr.client())
    );
    let transfer_meta = TransferProtocolMeta::new(1, network_mgr.get_sender());

    let mut service = ServiceBuilder::default()
        .insert_protocol(discovery_meta)
        .insert_protocol(transfer_meta)
        .forever(true)
        .key_pair(SecioKeyPair::secp256k1_generated())
        .build(SHandle::new(nodes_mgr.client()));
    let addr = format!("/ip4/127.0.0.1/tcp/{}", config.port.unwrap_or(DEFAULT_PORT));
    let _ = service.listen(&addr.parse().unwrap());
    nodes_mgr.set_service_task_sender(service.control().clone());
    // <<<< End init p2p protocols

    // >>>> Run system


    // Thread for handle new transactions from MQ
    let nodes_manager_client = nodes_mgr.client();
    thread::spawn(move || loop {
        let (key, body) = crx_sub_auth.recv().unwrap();
        let msg = Message::try_from(&body).unwrap();

        // Broadcast the message to other nodes
        nodes_manager_client.broadcast(BroadcastReq::new(key, msg));
    });

    thread::spawn(move || nodes_mgr.run());
    thread::spawn(move || network_mgr.run());
    tokio::run(service.for_each(|_| Ok(())));
    // <<<< End run system
}
