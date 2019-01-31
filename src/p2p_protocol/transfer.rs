
use log::{ info };
use p2p::{
    ProtocolId,
    traits::{
        ProtocolMeta, ServiceProtocol,
    },
    context::{
        ServiceContext, SessionContext,
    },
    SessionId,
};
use libproto::{
    Message as ProtoMessage,
    TryInto, TryFrom
};
use bytes::BytesMut;
use tokio::codec::length_delimited::LengthDelimitedCodec;
use crate::network::{ NetworkClient, RemoteMessage };
use crate::citaprotocol::network_message_to_pubsub_message;

pub struct TransferProtocolMeta {
    id: ProtocolId,
    network_client: NetworkClient,
}

impl TransferProtocolMeta {
    pub fn new(id: ProtocolId, network_client: NetworkClient) -> Self {
        TransferProtocolMeta {
            id,
            network_client,
        }
    }
}

impl ProtocolMeta<LengthDelimitedCodec> for TransferProtocolMeta {
    fn id(&self) -> ProtocolId {
        self.id
    }
    fn codec(&self) -> LengthDelimitedCodec {
        LengthDelimitedCodec::new()
    }
    fn service_handle(&self) -> Option<Box<dyn ServiceProtocol + Send + 'static>> {
        let handle = Box::new( TransferProtocol {
            proto_id: self.id,
            connected_session_ids: Vec::default(),
            network_client: self.network_client.clone(),
        });
        Some(handle)
    }
}

struct TransferProtocol {
    proto_id: ProtocolId,
    connected_session_ids: Vec<SessionId>,
    network_client: NetworkClient,
}

impl ServiceProtocol for TransferProtocol {
    fn init(&mut self, _control: &mut ServiceContext) {

    }

    fn connected(&mut self, _control: &mut ServiceContext, session: &SessionContext, version: &str) {
        info!("[connected] proto id [{}] open on session [{}], address: [{}], type: [{:?}], version: {}",
            self.proto_id, session.id, session.address, session.ty, version
        );
        self.connected_session_ids.push(session.id);
        info!("[connected] connected sessions: {:?}", self.connected_session_ids);
    }

    fn disconnected(&mut self, _control: &mut ServiceContext, session: &SessionContext) {
        let new_list = self
            .connected_session_ids
            .iter()
            .filter(|&id| id != &session.id)
            .cloned()
            .collect();
        self.connected_session_ids = new_list;

        info!("[disconnected] proto id [{}] close on session [{}]",
            self.proto_id, session.id
        );
    }

    fn received(&mut self, _env: &mut ServiceContext, session: &SessionContext, data: Vec<u8>) {
        let mut data = BytesMut::from(data);
        if let Some((key, message)) = network_message_to_pubsub_message(&mut data) {
            let mut msg = ProtoMessage::try_from(&message).unwrap();
            msg.set_origin(session.id as u32);
            self.network_client.handle_remote_message(RemoteMessage::new(
                key,
                msg.try_into().unwrap()
            ));
        }
    }
}
