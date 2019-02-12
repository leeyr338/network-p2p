use crate::node_manager::{
    AddConnectedNodeReq, DelConnectedNodeReq, DelNodeReq, NodesManagerClient,
};
use log::{debug, warn};
use p2p::{
    context::ServiceContext,
    error,
    service::{ServiceError, ServiceEvent},
    traits::ServiceHandle,
    utils::multiaddr_to_socketaddr,
    SessionType,
};

pub mod node_discovery;
pub mod transfer;

// This handle will be shared with all protocol
pub struct SHandle {
    nodes_mgr_client: NodesManagerClient,
}

impl SHandle {
    pub fn new(nodes_mgr_client: NodesManagerClient) -> Self {
        SHandle { nodes_mgr_client }
    }
}

impl ServiceHandle for SHandle {
    fn handle_error(&mut self, _env: &mut ServiceContext, error: ServiceError) {
        debug!("return error {:?}", error);
        match error {
            ServiceError::DialerError { address, error } => {
                let address = multiaddr_to_socketaddr(&address).unwrap();

                // If dial to a connected node, need add it to connected address list.
                match error {
                    error::Error::RepeatedConnection(session_id) => {
                        let req = AddConnectedNodeReq::new(address, session_id);
                        self.nodes_mgr_client.add_connected_node(req);
                        debug!("[handle_error] Connected to the same node : {:?}", address);
                    }
                    _ => {
                        //FIXME: Using score for deleting a node from known nodes
                        let req = DelNodeReq::new(address);
                        self.nodes_mgr_client.del_node(req);
                        warn!("[handle_error] Error in {:?} : {:?}, delete this address from nodes manager",
                              address, error);
                    }
                }
            }
            ServiceError::ListenError { address, error } => {
                let address = multiaddr_to_socketaddr(&address).unwrap();
                warn!("Listen error on {:?}, error info: {:?}", address, error);
            }
        }
    }

    fn handle_event(&mut self, _env: &mut ServiceContext, event: ServiceEvent) {
        match event {
            ServiceEvent::SessionOpen {
                id,
                address,
                ty,
                public_key,
            } => {
                let address = multiaddr_to_socketaddr(&address).unwrap();
                debug!("[handle_event] Service open on : {:?}, session id: {:?}, ty: {:?}, public_key: {:?}",
                       address, id, ty, public_key);
                if ty == SessionType::Client {
                    let req = AddConnectedNodeReq::new(address, id);
                    self.nodes_mgr_client.add_connected_node(req);
                }
            }
            ServiceEvent::SessionClose { id } => {
                let req = DelConnectedNodeReq::new(id);
                self.nodes_mgr_client.del_connected_node(req);
            }
        }
    }
}
