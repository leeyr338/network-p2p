
use log::info;
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

use tokio::codec::length_delimited::LengthDelimitedCodec;

pub struct TransferProtocolMeta {
    id: ProtocolId,
}

impl TransferProtocolMeta {
    fn new(id: ProtocolId) -> Self {
        TransferProtocolMeta { id }
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
        });
        Some(handle)
    }
}

#[derive(Default)]
struct TransferProtocol {
    proto_id: ProtocolId,
    connected_session_ids: Vec<SessionId>,
}

impl ServiceProtocol for TransferProtocol {
    fn init(&mut self, control: &mut ServiceContext) {

    }

    fn connected(&mut self, control: &mut ServiceContext, session: &SessionContext, version: &str) {
        info!("[connected] proto id [{}] open on session [{}], address: [{}], type: [{:?}], version: {}",
            self.proto_id, session.id, session.address, session.ty, version
        );
        self.connected_session_ids.push(session.id);
        info!("[connected] connected sessions: {:?}", self.connected_session_ids);
    }

    fn disconnected(&mut self, control: &mut ServiceContext, session: &SessionContext) {
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

    fn received(&mut self, env: &mut ServiceContext, session: &SessionContext, data: Vec<u8>) {

    }

    fn notify(&mut self, control: &mut ServiceContext, token: u64) {
        info!("[notify] proto [{}] received notify, token: {}", self.proto_id, token);
    }
}