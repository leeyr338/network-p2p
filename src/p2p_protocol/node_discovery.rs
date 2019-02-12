use crate::node_manager::{AddNodeReq, GetRandomNodesReq, NodesManagerClient};
use crossbeam_channel;
use crossbeam_channel::unbounded;
use discovery::{AddressManager, Direction, Discovery, DiscoveryHandle, Substream};
use fnv::FnvHashMap;
use futures::{
    prelude::*,
    sync::mpsc::{channel, Sender},
};
use log::{debug, warn};
use p2p::{
    context::{ServiceContext, SessionContext},
    multiaddr::{Multiaddr, ToMultiaddr},
    traits::{ProtocolMeta, ServiceProtocol},
    utils::multiaddr_to_socketaddr,
    ProtocolId, SessionId, SessionType,
};
use tokio::codec::length_delimited::LengthDelimitedCodec;

#[derive(Clone, Debug)]
pub struct NodesAddressManager {
    pub nodes_mgr_client: NodesManagerClient,
}

impl NodesAddressManager {
    pub fn new(nodes_mgr_client: NodesManagerClient) -> Self {
        NodesAddressManager { nodes_mgr_client }
    }
}

impl AddressManager for NodesAddressManager {
    fn add_new(&mut self, addr: Multiaddr) {
        let address = multiaddr_to_socketaddr(&addr).unwrap();
        let req = AddNodeReq::new(address);
        self.nodes_mgr_client.add_node(req);

        debug!("[add_new] Add node {:?} to manager", address);
    }

    fn misbehave(&mut self, _addr: Multiaddr, _ty: u64) -> i32 {
        unimplemented!()
    }

    fn get_random(&mut self, n: usize) -> Vec<Multiaddr> {
        let (tx, rx) = unbounded();

        let req = GetRandomNodesReq::new(n, tx);
        self.nodes_mgr_client.get_random_nodes(req);

        let ret = rx.recv().unwrap();

        debug!("[get_random] Get address : {:?} from nodes manager.", ret);

        ret.into_iter()
            .map(|addr| addr.to_multiaddr().unwrap())
            .collect()
    }
}

pub struct DiscoveryProtocol {
    id: usize,
    discovery: Option<Discovery<NodesAddressManager>>,
    discovery_handle: DiscoveryHandle,
    discovery_senders: FnvHashMap<SessionId, Sender<Vec<u8>>>,
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
                    })
                    .then(|_| {
                        warn!("End of discovery");
                        Ok(())
                    })
            })
            .unwrap();
        let _ = control.future_task(discovery_task);
    }

    // open a discovery protocol session?
    fn connected(&mut self, control: &mut ServiceContext, session: &SessionContext, _: &str) {
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

    fn disconnected(&mut self, _control: &mut ServiceContext, session: &SessionContext) {
        self.discovery_senders.remove(&session.id);
        debug!("protocol [discovery] close on session [{}]", session.id);
    }

    fn received(&mut self, _control: &mut ServiceContext, session: &SessionContext, data: Vec<u8>) {
        debug!("[received message]: length={}", data.len());
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
}

pub struct DiscoveryProtocolMeta {
    pub id: ProtocolId,
    pub addr_mgr: NodesAddressManager,
}

impl DiscoveryProtocolMeta {
    pub fn new(id: ProtocolId, addr_mgr: NodesAddressManager) -> Self {
        DiscoveryProtocolMeta { id, addr_mgr }
    }
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
            discovery: Some(discovery),
            discovery_handle,
            discovery_senders: FnvHashMap::default(),
        });

        Some(handle)
    }
}
