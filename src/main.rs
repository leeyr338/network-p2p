
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
    DiscoveryProtocolMeta, SHandle, NodesAddressManager, DEFAULT_PORT, DEFAULT_KNOWN_NODES,
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

    if std::env::args().nth(1) == Some("server".to_string()) {
        debug!("Starting server ......");
        let mut node_mgr = create_nodes_manager(1);

        let protocol_meta= create_meta(node_mgr.get_sender(), 0);
        let mut service = ServiceBuilder::default()
            .insert_protocol(protocol_meta)
            .forever(true)
            .key_pair(SecioKeyPair::secp256k1_generated())
            .build(SHandle::new(node_mgr.get_sender()));
        let _ = service.listen(&"/ip4/0.0.0.0/tcp/1337".parse().unwrap());

        thread::spawn(move || node_mgr.run());
        tokio::run(service.for_each(|_| Ok(())));

    } else {
        debug!("Starting client ......");

        let config_path = std::env::args().nth(2).unwrap_or("/Volumes/x/cryptape/gettingStart/rust/network-p2p/examples/config/config.toml".to_string());
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
        let addr = format!("/ip4/0.0.0.0/tcp/{}", config.port.unwrap_or(DEFAULT_PORT));

        let _ = service.listen(&addr.parse().unwrap());

        thread::spawn(move || node_mgr.run());
        service = service.dial("/ip4/0.0.0.0/tcp/1337".parse().unwrap());
        tokio::run(service.for_each(|_| Ok(())));
    }
    println!("Hello, world!");
}

fn create_nodes_manager(start: u16) -> NodesManager {
    let addrs: FnvHashMap<RawAddr, i32> = (start..start + 3)
        .map(|port| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port))
        .map(|addr| (RawAddr::from(addr), 100))
        .collect();

    NodesManager::new(addrs)

}

fn create_meta(sender: crossbeam_channel::Sender<NodesManagerData>, id: usize) -> DiscoveryProtocolMeta {

    let addr_mgr = NodesAddressManager{ nodes_mgr_sender: sender };

    DiscoveryProtocolMeta { id, addr_mgr }
}