
pub mod config;
pub mod p2p_protocol;
pub mod node_manager;
pub mod network;

use env_logger;
use log::{debug};
use futures::{
    prelude::*,
};
use std::{
    thread,
};
use crossbeam_channel;

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
    NodesManager, DEFAULT_PORT,
};

use crate::network::{
    Network,
};

//include!(concat!(env!("OUT_DIR"), "./build_info.rs"));

fn main() {
    env_logger::init();

    let config_path = std::env::args().nth(1)
        .unwrap_or("/Volumes/x/cryptape/gettingStart/rust/network-p2p/examples/config/1_node.toml".to_string());
    debug!("config path {:?}", config_path);
    let config = NetConfig::new(&config_path);

    debug!("network config is {:?}", config);

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
    thread::spawn(move || nodes_mgr.run());
    thread::spawn(move || network_mgr.run());
    tokio::run(service.for_each(|_| Ok(())));
}
