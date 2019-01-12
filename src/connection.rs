
use log::{debug, warn};
use std::{
    net::{SocketAddr},
    collections::HashMap,
    time::Duration,
};
use futures::{
    prelude::*,
    sync::mpsc::{channel, Sender},
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

#[derive(Default, Clone, Debug)]
pub struct NodesAddressManager {

    //FIXME: It is better to use a channel?
    pub addrs: FnvHashMap<RawAddr, i32>,
}

impl AddressManager for NodesAddressManager {
    fn add_new(&mut self, addr: SocketAddr) {

        // Question: why this insert 100?
        debug!("add node {:?} to manager", addr);
        self.addrs.entry(RawAddr::from(addr)).or_insert(100);
    }

    fn misbehave(&mut self, addr: SocketAddr, ty: u64) -> i32 {
        let value = self.addrs.entry(RawAddr::from(addr)).or_insert(100);
        *value -= 20;
        *value
    }

    fn get_random(&mut self, n: usize) -> Vec<SocketAddr> {
        self.addrs
            .keys()
            .take(n)
            .map(|addr| addr.socket_addr())
            .collect()
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
