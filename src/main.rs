
pub mod connection;
pub mod config;

use env_logger;
use fnv::FnvHashMap;
use log::{debug};
use futures::{
    prelude::*,
};
use std::{
    net::{SocketAddr, IpAddr, Ipv4Addr},
    thread,
};
use crossbeam_channel;
use futures::{
    prelude::*,
    sync::mpsc::{channel, Sender, Receiver},
};
use clap::App;
use util::micro_service_init;
use discovery::{RawAddr};
use crate::config::NetConfig;
use crate::connection::{
    DiscoveryProtocolMeta, SHandle, NodesAddressManager, DEFAULT_PORT,
    NodesManager, NodesManagerData,
};

use p2p::{
    multiaddr::{ Multiaddr, ToMultiaddr },
    builder::ServiceBuilder,
    SecioKeyPair,
};

//include!(concat!(env!("OUT_DIR"), "./build_info.rs"));

fn main() {
    env_logger::init();

    let config_path = std::env::args().nth(1).unwrap_or("/Volumes/x/cryptape/gettingStart/rust/network-p2p/examples/config/1_node.toml".to_string());
    debug!("config path {:?}", config_path);
    let config = NetConfig::new(&config_path);

    debug!("network config is {:?}", config);

    let mut node_mgr = NodesManager::from_config(config.clone());

    let protocol_meta= create_meta(node_mgr.get_sender(), 0);

    let mut service = ServiceBuilder::default()
        .insert_protocol(protocol_meta)
        .forever(true)
        .key_pair(SecioKeyPair::secp256k1_generated())
        .build(SHandle::new(node_mgr.get_sender()));
    let addr = format!("/ip4/127.0.0.1/tcp/{}", config.port.unwrap_or(DEFAULT_PORT));

    let _ = service.listen(&addr.parse().unwrap());

    node_mgr.set_service_task_sender(service.control().clone());
    thread::spawn(move || node_mgr.run());
    tokio::run(service.for_each(|_| Ok(())));
}

fn create_meta(sender: crossbeam_channel::Sender<NodesManagerData>, id: usize) -> DiscoveryProtocolMeta {

    let addr_mgr = NodesAddressManager{ nodes_mgr_sender: sender };

    DiscoveryProtocolMeta { id, addr_mgr }
}