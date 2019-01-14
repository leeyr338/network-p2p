
use log::{debug, warn};
use std::{
    net::{SocketAddr},
    collections::HashMap,
    time::Duration,
};
use futures::{
    prelude::*,
    sync::mpsc::{channel, Sender, Receiver},
};
use fnv::FnvHashMap;
use tokio::codec::length_delimited::LengthDelimitedCodec;
use discovery::{RawAddr, AddressManager, Discovery, DiscoveryHandle, Direction, Substream};
use p2p::{
    service::{
        ProtocolHandle, ServiceHandle, ServiceContext, ServiceEvent, ProtocolMeta, ServiceProtocol,
        SessionContext,
    },
    session::{SessionId, ProtocolId},
    SessionType,
};

pub const DEFAULT_KNOWN_NODES: &str = "0.0.0.0:1337";
pub const DEFAULT_MAX_CONNECTS: usize = 4;
pub const DEFAULT_PORT: usize = 4000;

pub struct NodesManager {
    known_addrs: FnvHashMap<RawAddr, i32>,
    connected_addrs: FnvHashMap<RawAddr, i32>,
    max_connects: usize,
    add_node_sender: Sender<NodesManagerData>,
    add_node_receiver: Receiver<NodesManagerData>,
}

impl NodesManager {
    pub fn new(known_addrs: FnvHashMap<RawAddr, i32>) -> Self {
        let (tx, rx) = channel(8);

        NodesManager {
            known_addrs,
            connected_addrs: FnvHashMap::default(),
            max_connects: DEFAULT_MAX_CONNECTS,
            add_node_sender: tx,
            add_node_receiver: rx,
        }
    }

    pub fn get_sender(&self) -> Sender<NodesManagerData> {
        self.add_node_sender.clone()
    }
}

impl Default for NodesManager {
    fn default() -> NodesManager {
        let (tx, rx) = channel(8);

        NodesManager {
            known_addrs: FnvHashMap::default(),
            connected_addrs: FnvHashMap::default(),
            max_connects: DEFAULT_MAX_CONNECTS,
            add_node_sender: tx,
            add_node_receiver: rx,
        }
    }
}

#[derive(Debug)]
pub enum ManagerCmd {
    ADD_ADDRESS,
    DEL_ADDRESS,
    GET_RANDOM,
}

#[derive(Debug)]
pub struct GetRandomAddrData {
    num: usize,

    // After get the random address, use data_channel sender them back immediately
    data_channel: Sender<Vec<SocketAddr>>,
}

#[derive(Debug)]
pub struct NodesManagerData {
    cmd: ManagerCmd,
    addr: Option<SocketAddr>,
    get_random: Option<GetRandomAddrData>,

}


#[derive(Clone, Debug)]
pub struct NodesAddressManager {
    pub nodes_mgr_sender: Sender<NodesManagerData>,
}

impl AddressManager for NodesAddressManager {
    fn add_new(&mut self, addr: SocketAddr) {

        // Question: why this insert 100?
        debug!("add node {:?}:{} to manager", addr, addr.port());

        let data = NodesManagerData {
            cmd: ManagerCmd::ADD_ADDRESS,
            addr: Some(addr),
            get_random: None,
        };

        match self.nodes_mgr_sender.try_send(data) {
            Ok(_) => {
                debug!("Send substream success");
            }
            Err(err) => {
                warn!("Send substream failed : {:?}", err);
            }
        }
    }

    // Question: why we need this?
    fn misbehave(&mut self, addr: SocketAddr, ty: u64) -> i32 {
        unimplemented!()
    }

    fn get_random(&mut self, n: usize) -> Vec<SocketAddr> {
        let (tx, mut rx) = channel(8);
        let data = NodesManagerData {
            cmd: ManagerCmd::GET_RANDOM,
            addr: None,
            get_random: Some(GetRandomAddrData {num: n, data_channel: tx}),
        };

        match self.nodes_mgr_sender.try_send(data) {
            Ok(_) => {
                debug!("Send substream success");
            }
            Err(err) => {
                warn!("Send substream failed : {:?}", err);
            }
        }

        let mut ret = Vec::default();
        if let Ok(Async::Ready(Some(t))) = rx.poll() {
            ret = t;
        }
        ret
    }
}

// This handle will be shared with all protocol
pub struct SHandle {
    nodes_mgr_sender: Sender<NodesManagerData>,
}

impl SHandle {
    pub fn new(sender: Sender<NodesManagerData>) -> Self {
        SHandle {
            nodes_mgr_sender: sender,
        }
    }
}

impl ServiceHandle for SHandle {

    // FIXME : when connect error, remove the node from node manager.
    fn handle_error(&mut self, env: &mut ServiceContext, error: ServiceEvent) {
        debug!("service error: {:?}", error);
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

impl SessionData {
    fn new(address: SocketAddr, ty: SessionType) -> Self {
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
}

impl ServiceProtocol for DiscoveryProtocol {
    fn init(&mut self, control: &mut ServiceContext) {
        debug!("protocol [discovery({})]: init", self.id);

        let interval = Duration::from_secs(5);
        debug!("Setup interval {:?}", interval);

        // why we need a notify?
        control.set_notify(self.id, interval, 3);
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
            .or_insert(SessionData::new(session.address, session.ty));
        debug!(
            "protocol [discovery] open session [{}], address: [{}], type: [{:?}]",
            session.id, session.address, session.ty
        );

        let direction = if session.ty == SessionType::Server {
            Direction::Inbound
        } else {
            Direction::Outbound
        };

        let (sender, receiver) = channel(8);
        self.discovery_senders.insert(session.id, sender);

        let substream = Substream::new(
            session.address,
            direction,
            self.id,
            session.id,
            receiver,
            control.sender().clone(),
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
        });

        Some(handle)
    }
}
