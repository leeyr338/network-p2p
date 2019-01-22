
use log::{debug, warn};
use std::{
    net::{ SocketAddr },
    collections::HashMap,
    time::{ Duration, Instant },
    str::FromStr,

};
use futures::{
    sync::mpsc::{ Sender },
};
use crossbeam_channel;
use crossbeam_channel::{
    unbounded, tick, select,
};
use fnv::FnvHashMap;
use discovery::{RawAddr};
use p2p::{
    SessionId,
    context::{
        ServiceControl,
    },
    multiaddr::{
        ToMultiaddr
    },
    service::{
        ServiceTask, Message,
    },
};
use crate::config::{
    NetConfig, NodeConfig,
};


pub const DEFAULT_KNOWN_IP: &str = "127.0.0.1";
pub const DEFAULT_KNOWN_PORT: usize = 1337;
pub const DEFAULT_MAX_CONNECTS: usize = 4;
pub const DEFAULT_PORT: usize = 4000;
pub const CHECK_CONNECTED_NODES: Duration = Duration::from_secs(3);

pub struct NodesManager {
    check_connected_nodes: crossbeam_channel::Receiver<Instant>,
    known_addrs: FnvHashMap<RawAddr, i32>,
    connected_addrs: HashMap<SessionId, RawAddr>,
    max_connects: usize,
    add_node_sender: crossbeam_channel::Sender<NodesManagerData>,
    add_node_receiver: crossbeam_channel::Receiver<NodesManagerData>,
    service_ctrl: Option<ServiceControl>,
}

impl NodesManager {
    pub fn new(known_addrs: FnvHashMap<RawAddr, i32>) -> Self {
        let mut node_mgr = NodesManager::default();
        node_mgr.known_addrs = known_addrs;
        node_mgr
    }

    // FIXME: handle the error
    pub fn from_config(cfg: NetConfig) -> Self {
        let mut node_mgr = NodesManager::default();

        let cfg_addrs = cfg.known_nodes
            .unwrap_or(vec!(NodeConfig {
                ip: Some(DEFAULT_KNOWN_IP.to_owned()),
                port: Some(DEFAULT_KNOWN_PORT),
            }));

        let max_connects = cfg.max_connects.unwrap();
        node_mgr.max_connects = max_connects;

        for addr in cfg_addrs {
            let addr_str = format!("{}:{}", addr.ip.unwrap(), addr.port.unwrap());
            let socket_addr = SocketAddr::from_str(&addr_str).unwrap();
            let raw_addr = RawAddr::from(socket_addr);
            node_mgr.known_addrs.insert(raw_addr, 100);
        }

        node_mgr

    }

    pub fn run(&mut self) {

        loop {
            select! {
                recv(self.add_node_receiver) -> msg => {
                    match msg {
                        Ok(data) => {
                            match data.cmd {
                                ManagerCmd::AddAddress => {
                                    let addr = data.addr.unwrap();
                                    self.known_addrs.entry(RawAddr::from(addr)).or_insert(100);
                                },
                                ManagerCmd::DelAddress => {

                                    // Connected failed!
                                    let addr = data.addr.unwrap();
                                    self.known_addrs.remove(&RawAddr::from(addr));
                                },
                                ManagerCmd::GetRandom => {
                                    let get_random_data = data.get_random.unwrap();
                                    let n = get_random_data.num;
                                    let addrs = self.known_addrs
                                        .keys()
                                        .take(n)
                                        .map(|addr| addr.socket_addr())
                                        .collect();
                                    let sender = get_random_data.data_channel;
                                    match sender.try_send(addrs) {
                                        Ok(_) => {
                                            debug!("Get random n addresses and send them success");
                                        }
                                        Err(err) => {
                                            warn!("Get random n addresses, send them failed : {:?}", err);
                                        }
                                    }
                                },
                                ManagerCmd::AddConnected => {
                                    //FIXME: If have reached to max_connects, deconnected this node.
                                    // Connect success!
                                    let addr = data.addr.unwrap();
                                    let id = data.session_id.unwrap_or(1);
                                    self.connected_addrs.insert(id, RawAddr::from(addr));
                                }
                                ManagerCmd::DelConnected => {
                                    //FIXME: If have reached to max_connects, deconnected this node.
                                    // Connect success!
                                    let id = data.session_id.unwrap_or(1);
                                    self.connected_addrs.remove(&id);
                                }
                            }
                        },
                        Err(err) => debug!("Err {:?}", err),
                    }
                }
                recv(self.check_connected_nodes) -> _ => {
                    self.dial_nodes();
                }
            }

        }
    }

    pub fn get_sender(&self) -> crossbeam_channel::Sender<NodesManagerData> {
        self.add_node_sender.clone()
    }

    pub fn dial_nodes(&self) {
        // FIXME: If there are no addrs in known_addrs, dial a default address.
        debug!("=============================");
        for raw_addr in self.known_addrs.keys() {
            debug!("Address in known: {:?}", raw_addr.socket_addr());
        }
        debug!("-----------------------------");
        for raw_addr in self.connected_addrs.values() {
            debug!("Address in connected: {:?}", raw_addr.socket_addr());
        }
        debug!("=============================");

        let  msg = Message {
            session_id: 0,
            proto_id: 1,
            data: b"Hello world".to_vec(),
        };
        self.service_ctrl.clone().unwrap().send_message(None, msg);
        if self.connected_addrs.len() < self.max_connects {
            for key in self.known_addrs.keys() {
                if false == self.connected_addrs.values().any(|value| *value == *key) {
                    debug!("[dial_nodes] Connect to {:?}", key.socket_addr());

                    // FIXME: Do not use ctrl.clone later.
                    if let Some(ctrl) = &self.service_ctrl {
                        match ctrl.clone().dial(key.socket_addr().to_multiaddr().unwrap()) {
                            Ok(_) => {
                                debug!("[dial_nodes] Dail success");
                            }
                            Err(err) => {
                                warn!("[dial_nodes] Dail failed : {:?}", err);
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    pub fn set_service_task_sender(&mut self, ctrl: ServiceControl) {
        self.service_ctrl = Some(ctrl);
    }
}

impl Default for NodesManager {
    fn default() -> NodesManager {
        let (tx, rx) = unbounded();
        let ticker = tick(CHECK_CONNECTED_NODES);
        NodesManager {
            check_connected_nodes: ticker,
            known_addrs: FnvHashMap::default(),
            connected_addrs: HashMap::default(),
            max_connects: DEFAULT_MAX_CONNECTS,
            add_node_sender: tx,
            add_node_receiver: rx,
            service_ctrl: None,
        }
    }
}

#[derive(Debug)]
pub enum ManagerCmd {
    AddAddress,
    DelAddress,
    GetRandom,
    AddConnected,
    DelConnected,
}

#[derive(Debug)]
pub struct GetRandomAddrData {
    pub num: usize,

    // After get the random address, use data_channel sender them back immediately
    pub data_channel: crossbeam_channel::Sender<Vec<SocketAddr>>,
}

//#[derive(Debug)]
pub struct NodesManagerData {
    pub cmd: ManagerCmd,
    pub addr: Option<SocketAddr>,
    pub get_random: Option<GetRandomAddrData>,
    pub service_task_sender: Option<Sender<ServiceTask>>,
    pub session_id: Option<SessionId>,
}