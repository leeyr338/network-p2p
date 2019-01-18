
use log::{debug, warn};
use std::{
    net::{ SocketAddr },
    collections::HashMap,
    time::{ Duration, Instant },
    str::FromStr,
    io::{Error, ErrorKind},

};
use futures::{
    prelude::*,
    sync::mpsc::{channel, Sender, Receiver},
};
use crossbeam_channel;
use crossbeam_channel::{
    unbounded, bounded, tick, select,
};
use fnv::FnvHashMap;
use tokio::codec::length_delimited::LengthDelimitedCodec;
use discovery::{RawAddr, AddressManager, Discovery, DiscoveryHandle, Direction, Substream};
use p2p::{
    ProtocolId, SessionId, SessionType,
    context::{
        ServiceContext, SessionContext, ServiceControl,
    },
    multiaddr::{
        Multiaddr, ToMultiaddr
    },
    traits::{
        ProtocolMeta, ServiceProtocol, ServiceHandle,
    },
    utils::multiaddr_to_socketaddr,
    service::{
        ServiceEvent, ServiceTask,
    },
    error,
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
                                ManagerCmd::ADD_ADDRESS => {
                                    let addr = data.addr.unwrap();
                                    self.known_addrs.entry(RawAddr::from(addr)).or_insert(100);
                                },
                                ManagerCmd::DEL_ADDRESS => {

                                    // Connected failed!
                                    let addr = data.addr.unwrap();
                                    self.known_addrs.remove(&RawAddr::from(addr));
                                },
                                ManagerCmd::GET_RANDOM => {
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
                                ManagerCmd::ADD_CONNECTED => {
                                    //FIXME: If have reached to max_connects, deconnected this node.
                                    // Connect success!
                                    let addr = data.addr.unwrap();
                                    let id = data.session_id.unwrap_or(1);
                                    self.connected_addrs.insert(id, RawAddr::from(addr));
                                }
                                ManagerCmd::DEL_CONNECTED => {
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
    ADD_ADDRESS,
    DEL_ADDRESS,
    GET_RANDOM,
    ADD_CONNECTED,
    DEL_CONNECTED,
}

#[derive(Debug)]
pub struct GetRandomAddrData {
    num: usize,

    // After get the random address, use data_channel sender them back immediately
    data_channel: crossbeam_channel::Sender<Vec<SocketAddr>>,
}

//#[derive(Debug)]
pub struct NodesManagerData {
    cmd: ManagerCmd,
    addr: Option<SocketAddr>,
    get_random: Option<GetRandomAddrData>,
    service_task_sender: Option<Sender<ServiceTask>>,
    session_id: Option<SessionId>,
}


#[derive(Clone, Debug)]
pub struct NodesAddressManager {
    pub nodes_mgr_sender: crossbeam_channel::Sender<NodesManagerData>,
}

impl AddressManager for NodesAddressManager {
    fn add_new(&mut self, addr: Multiaddr) {
        let addr = multiaddr_to_socketaddr(&addr).unwrap();
        debug!("add node {:?}:{} to manager", addr, addr.port());

        let data = NodesManagerData {
            cmd: ManagerCmd::ADD_ADDRESS,
            addr: Some(addr),
            get_random: None,
            service_task_sender: None,
            session_id: None,
        };

        match self.nodes_mgr_sender.try_send(data) {
            Ok(_) => {
                debug!("Send new address to nodes manager success");
            }
            Err(err) => {
                warn!("Send new address to nodes manager failed : {:?}", err);
            }
        }
    }

    // Question: why we need this?
    fn misbehave(&mut self, addr: Multiaddr, ty: u64) -> i32 {
        unimplemented!()
    }

    fn get_random(&mut self, n: usize) -> Vec<Multiaddr> {
        let (tx, mut rx) = unbounded();
        let data = NodesManagerData {
            cmd: ManagerCmd::GET_RANDOM,
            addr: None,
            get_random: Some(GetRandomAddrData {num: n, data_channel: tx}),
            service_task_sender: None,
            session_id: None,
        };

        match self.nodes_mgr_sender.try_send(data) {
            Ok(_) => {
                debug!("Send message to address manager to get n random address Success");
            }
            Err(err) => {
                warn!("Send message to address manager to get n random address failed : {:?}", err);
            }
        }

        let mut ret: Vec<SocketAddr> = Vec::default();
        let ret = rx.recv().unwrap();
        debug!("Get address : {:?}", ret);
        ret.into_iter()
            .map(|addr| addr.to_multiaddr().unwrap())
            .collect()

    }
}

// This handle will be shared with all protocol
pub struct SHandle {
    nodes_mgr_sender: crossbeam_channel::Sender<NodesManagerData>,
}

impl SHandle {
    pub fn new(sender: crossbeam_channel::Sender<NodesManagerData>) -> Self {
        SHandle {
            nodes_mgr_sender: sender,
        }
    }
}

impl ServiceHandle for SHandle {

    fn handle_error(&mut self, env: &mut ServiceContext, error: ServiceEvent) {
        debug!("return error {:?}", error);
        match error {
            ServiceEvent::DialerError{ address, error } => {
                let address = multiaddr_to_socketaddr(&address).unwrap();

                // If dial to a connected node, need add it to connected address list.
                // FIXME: Use a new error kind to distinguish the `Connected to the connected node`
                match error {
                    error::Error::RepeatedConnection(session_id) => {
                        let data = NodesManagerData {
                            cmd: ManagerCmd::ADD_CONNECTED,
                            addr: Some(address),
                            get_random: None,
                            service_task_sender: None,
                            session_id: Some(session_id),
                        };
                        match self.nodes_mgr_sender.try_send(data) {
                            Ok(_) => {
                                debug!("[handle_event] Send message to address manager to delete address Success");
                            }
                            Err(err) => {
                                warn!("[handle_envent] Send message to address manager to delete address failed : {:?}", err);
                            }
                        }

                        debug!("Connected to the same node : {:?}", address);
                    },
                    _ => {
                        let data = NodesManagerData {
                            cmd: ManagerCmd::DEL_ADDRESS,
                            addr: Some(address),
                            get_random: None,
                            service_task_sender: None,
                            session_id: None,
                        };
                        match self.nodes_mgr_sender.try_send(data) {
                            Ok(_) => {
                                debug!("Send message to address manager to delete address Success");
                            }
                            Err(err) => {
                                warn!("Send message to address manager to delete address failed : {:?}", err);
                            }
                        }
                        warn!("Error in {:?} : {:?}, delete this address from nodes manager", address, error);
                    }
                }
            },

            _ => (),
        }
    }

    // Question: this will be called every session open?
    fn handle_event(&mut self, env: &mut ServiceContext, event: ServiceEvent) {
        match event {
            ServiceEvent::SessionOpen {
                id,
                address,
                ty,
                public_key,
            } => {
                let address = multiaddr_to_socketaddr(&address).unwrap();
                debug!("[handle_event] Service open on : {:?}, session id: {:?}, ty: {:?}", address, id, ty);
                if ty == SessionType::Client {
                    debug!("[handle_event] ty == Client");
                    // FIXME: this logic should be a function
                    let data = NodesManagerData {
                        cmd: ManagerCmd::ADD_CONNECTED,
                        addr: Some(address),
                        get_random: None,
                        service_task_sender: None,
                        session_id: None,
                    };
                    match self.nodes_mgr_sender.try_send(data) {
                        Ok(_) => {
                            debug!("[handle_event] Send message to address manager to delete address Success");
                        }
                        Err(err) => {
                            warn!("[handle_envent] Send message to address manager to delete address failed : {:?}", err);
                        }
                    }
                }

            },
            _ => (),
        }
    }
}

#[derive(Clone)]
struct SessionData {
    ty: SessionType,
    address: Multiaddr,
    data: Vec<Vec<u8>>,
}

impl SessionData {
    fn new(address: Multiaddr, ty: SessionType) -> Self {
        SessionData {
            ty,
            address,
            data: Vec::new(),
        }
    }

    fn push_data(&mut self, data: Vec<u8>) {
        self.data.push(data);
    }
}

pub struct DiscoveryProtocol {
    id: usize,
    notify_counter: u32,
    discovery: Option<Discovery<NodesAddressManager>>,
    discovery_handle: DiscoveryHandle,
    discovery_senders: FnvHashMap<SessionId, Sender<Vec<u8>>>,
    sessions: HashMap<SessionId, SessionData>,
    nodes_mgr_sender: crossbeam_channel::Sender<NodesManagerData>,
}

impl ServiceProtocol for DiscoveryProtocol {
    fn init(&mut self, control: &mut ServiceContext) {
        debug!("protocol [discovery({})]: init", self.id);

        let discovery_task = self
            .discovery
            .take()
            .map(|discovery| {
                debug!("Start discovery future_task");
                discovery
                    .for_each(|()| {
                        debug!("discovery.for_each()");
                        Ok(())
                    })
                    .map_err(|err| {
                        warn!("discovery stream error: {:?}", err);
                        ()
                    })
                    .then(|_| {
                        warn!("End of discovery");
                        Ok(())
                    })
            })
            .unwrap();
        control.future_task(discovery_task);
    }

    // open a discovery protocol session?
    fn connected(&mut self, control: &mut ServiceContext, session: &SessionContext, _: &str) {
        self.sessions
            .entry(session.id)
            .or_insert(SessionData::new(session.address.clone(), session.ty));
        debug!(
            "protocol [discovery] open session [{}], address: [{}], type: [{:?}]",
            session.id, session.address, session.ty
        );

        debug!("listen list: {:?}", control.listens());
        let direction = if session.ty == SessionType::Server {
            Direction::Inbound
        } else {
            Direction::Outbound
        };

        let (sender, receiver) = channel(8);
        self.discovery_senders.insert(session.id, sender);

        let substream = Substream::new(
            &session.address,
            direction,
            self.id,
            session.id,
            receiver,
            control.control().clone(),
            control.listens(),
        );

        match self.discovery_handle.substream_sender.try_send(substream) {
            Ok(_) => {
                debug!("Send substream success");
            }
            Err(err) => {
                warn!("Send substream failed: {:?}", err);
            }
        }
    }

    fn disconnected(&mut self, control: &mut ServiceContext, session: &SessionContext) {
        self.sessions.remove(&session.id);
        self.discovery_senders.remove(&session.id);
        debug!("protocol [discovery] close on session [{}]", session.id);

        let address = multiaddr_to_socketaddr(&session.address).unwrap();
        debug!("[disconnected] address : {:?}", address);
        let data = NodesManagerData {
            cmd: ManagerCmd::DEL_CONNECTED,
            addr: Some(address),
            get_random: None,
            service_task_sender: None,
            session_id: Some(session.id),
        };
        match self.nodes_mgr_sender.try_send(data) {
            Ok(_) => {
                debug!("Send message to address manager to delete address Success");
            }
            Err(err) => {
                warn!("Send message to address manager to delete address failed : {:?}", err);
            }
        }
    }

    fn received(&mut self, control: &mut ServiceContext, session: &SessionContext, data: Vec<u8>) {
        debug!("[received message]: length={}", data.len());
        self.sessions
            .get_mut(&session.id)
            .unwrap()
            .push_data(data.clone());
        if let Some(ref mut sender) = self.discovery_senders.get_mut(&session.id) {
            if let Err(err) = sender.try_send(data) {
                if err.is_full() {
                    warn!("channel is full");
                } else if err.is_disconnected() {
                    warn!("channel is disconnected");
                } else {
                    warn!("other channel error {:?}", err);
                }
            }
        }

    }

    fn notify(&mut self, control: &mut ServiceContext, token: u64) {
        debug!("protocol [discovery] received notify token: {}", token);
        self.notify_counter += 1;
    }
}

pub struct DiscoveryProtocolMeta {
    pub id: ProtocolId,
    pub addr_mgr: NodesAddressManager,
}

impl ProtocolMeta<LengthDelimitedCodec> for DiscoveryProtocolMeta {
    fn id(&self) -> ProtocolId {
        self.id
    }

    fn codec(&self) -> LengthDelimitedCodec {
        LengthDelimitedCodec::new()
    }

    fn service_handle(&self) -> Option<Box<dyn ServiceProtocol + Send + 'static>> {
        let discovery = Discovery::new(self.addr_mgr.clone());
        let discovery_handle = discovery.handle();

        let handle = Box::new(DiscoveryProtocol {
            id: self.id,
            notify_counter: 0,
            discovery: Some(discovery),
            discovery_handle,
            discovery_senders: FnvHashMap::default(),
            sessions: HashMap::default(),
            nodes_mgr_sender: self.addr_mgr.nodes_mgr_sender.clone(),
        });

        Some(handle)
    }
}
