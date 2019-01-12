
use env_logger;
use fnv::FnvHashMap;
use log::{debug};
use futures::{
    prelude::*,
};
use std::{
    net::{SocketAddr, IpAddr, Ipv4Addr},
};
use discovery::{RawAddr};
mod connection;

use crate::connection::{DiscoveryProtocolMeta, SHandle, NodesAddressManager};


use p2p::{
    builder::ServiceBuilder,
};

fn main() {
    env_logger::init();
    if std::env::args().nth(1) == Some("server".to_string()) {
        debug!("Starting server ......");
        let protocol_meta = create_meta(1, 0);
        let mut service = ServiceBuilder::default()
            .insert_protocol(protocol_meta)
            .forever(true)
            .build(SHandle{});
        let _ = service.listen("127.0.0.1:1337".parse().unwrap());
        tokio::run(service.for_each(|_| Ok(())));
    } else {
        debug!("Starting client ......");
        let protocol_meta = create_meta(100, 0);
        let mut service = ServiceBuilder::default()
            .insert_protocol(protocol_meta)
            .forever(true)
            .build(SHandle{});
        let _ = service.listen("127.0.0.1:1338".parse().unwrap());
        let svc = service.dial("127.0.0.1:1337".parse().unwrap());
        tokio::run(svc.for_each(|_| Ok(())));
    }
    println!("Hello, world!");
}

fn create_meta(start: u16, id: usize) -> DiscoveryProtocolMeta {
    let addrs: FnvHashMap<RawAddr, i32> = (start..start + 10)
        .map(|port| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port))
        .map(|addr| (RawAddr::from(addr), 100))
        .collect();
    let addr_mgr = NodesAddressManager { addrs };
    DiscoveryProtocolMeta { id, addr_mgr }
}