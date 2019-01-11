
use log::{debug};
use std::{
    net::{SocketAddr},
    collections::HashMap,
};
use fnv::FnvHashMap;
use discovery::{RawAddr, AddressManager};
use p2p::{
    service::{ServiceHandle, ServiceContext, ServiceEvent},
    session::{SessionId},
    SessionType,
};

#[derive(Default, Clone, Debug)]
pub struct NodeManager {

    //FIXME: It is better to use a channel?
    pub addrs: FnvHashMap<RawAddr, i32>,
}

impl AddressManager for NodeManager {
    fn add_new(&mut self, addr: SocketAddr) {
        unimplemented!()
    }

    fn misbehave(&mut self, addr: SocketAddr, ty: u64) -> i32 {
        unimplemented!()
    }

    fn get_random(&mut self, n: usize) -> Vec<SocketAddr> {
        unimplemented!()
    }
}

// This handle will be shared with all protocol
pub struct SHandle{}

impl ServiceHandle for SHandle {

    // FIXME : when connect error, remove the node from node manager.
    fn handle_error(&mut self, env: &mut ServiceContext, error: ServiceEvent) {
        unimplemented!()
    }

    // Just a log here
    fn handle_event(&mut self, env: &mut ServiceContext, event: ServiceEvent) {
        debug!("service event: {:?}", event);
    }
}

#[derive(Clone)]
struct SessionData {
    ty: SessionType,
    address: SocketAddr,
    data: Vec<Vec<u8>>,
}

pub struct DiscoveryProtocol {
    sessions: HashMap<SessionId, SessionData>,
}

