

use log::{debug, warn};

use crossbeam_channel;

use p2p::{
    SessionType,
    context::{
        ServiceContext,
    },
    traits::{
        ServiceHandle,
    },
    utils::multiaddr_to_socketaddr,
    service::{
        ServiceEvent,
    },
    error,
};

use crate::node_manager::{
    ManagerCmd, NodesManagerData,
};

pub mod node_discovery;
pub mod transfer;

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

    fn handle_error(&mut self, _env: &mut ServiceContext, error: ServiceEvent) {
        debug!("return error {:?}", error);
        match error {
            ServiceEvent::DialerError{ address, error } => {
                let address = multiaddr_to_socketaddr(&address).unwrap();

                // If dial to a connected node, need add it to connected address list.
                // FIXME: Use a new error kind to distinguish the `Connected to the connected node`
                match error {
                    error::Error::RepeatedConnection(session_id) => {
                        let data = NodesManagerData {
                            cmd: ManagerCmd::AddConnected,
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
                            cmd: ManagerCmd::DelAddress,
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
    fn handle_event(&mut self, _env: &mut ServiceContext, event: ServiceEvent) {
        match event {
            ServiceEvent::SessionOpen {
                id,
                address,
                ty,
                public_key,
            } => {
                let address = multiaddr_to_socketaddr(&address).unwrap();
                debug!("[handle_event] Service open on : {:?}, session id: {:?}, ty: {:?}, publice_key: {:?}", address, id, ty, public_key);
                if ty == SessionType::Client {
                    debug!("[handle_event] ty == Client");
                    // FIXME: this logic should be a function
                    let data = NodesManagerData {
                        cmd: ManagerCmd::AddConnected,
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
            ServiceEvent::SessionClose { id } => {
                debug!("[handle_error] ========= session {} close! ", id);
                let data = NodesManagerData {
                    cmd: ManagerCmd::DelConnected,
                    addr: None,
                    get_random: None,
                    service_task_sender: None,
                    session_id: Some(id),
                };
                match self.nodes_mgr_sender.try_send(data) {
                    Ok(_) => {
                        debug!("Send message to address manager to delete address Success");
                    }
                    Err(err) => {
                        warn!("Send message to address manager to delete address failed : {:?}", err);
                    }
                }
            },
            _ => (),
        }
    }
}