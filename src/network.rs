
use log::{ debug, trace, error, info, warn };
use crossbeam_channel;
use crossbeam_channel::{
    unbounded,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use libproto::router::{
    RoutingKey, MsgType, SubModules,
};
use libproto::routing_key;
use libproto::{Message, Response};
use libproto::{TryFrom, TryInto};

pub struct Network {
    is_pause: Arc<AtomicBool>,
    msg_sender: crossbeam_channel::Sender<(Source, (String, Vec<u8>))>,
    msg_receiver: crossbeam_channel::Receiver<(Source, (String, Vec<u8>))>,
}

impl Network {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Network {
            is_pause: Arc::new(AtomicBool::new(false)),
            msg_sender: tx,
            msg_receiver: rx,
        }
    }

    pub fn get_sender(&self) -> crossbeam_channel::Sender<(Source, (String, Vec<u8>))> {
        self.msg_sender.clone()
    }

    pub fn run(&mut self) {
        loop {
            if let Ok((source, payload)) = self.msg_receiver.recv() {
                let (key, data) = payload;
                let rt_key = RoutingKey::from(&key);
                trace!("Network receive Message from {:?}/{}", source, key);

                if self.is_pause.load(Ordering::SeqCst) && rt_key.get_sub_module() != SubModules::Snapshot {
                    return;
                }
                match source {
                    // Message from MQ
                    Source::LOCAL => match rt_key {
                        routing_key!(Chain >> Status) => {
                            // FIXME: Send message to synchronizer
                        },
                        routing_key!(Chain >> SyncResponse) => {
                            let msg = Message::try_from(&data).unwrap();

                            // FIXME: Broadcast msg to other nodes
                        },
                        routing_key!(Jsonrpc >> RequestNet) => {
                            self.reply_rpc(&data);
                        },
                        routing_key!(Snapshot >> SnapshotReq) => {
                            info!("Set disconnect and response");
                            self.snapshot_req(&data);
                        },
                        _ => {
                            error!("Unexpected key {} from {:?}", key, source);
                        },
                    },

                    // Message from Other Nodes
                    Source::REMOTE => match rt_key {
                        routing_key!(Synchronizer >> Status)
                        | routing_key!(Synchronizer >> SyncResponse) => {
                            // FIXME: Forward data to synchronizer
                        },
                        routing_key!(Synchronizer >> SyncRequest) => {
                            // FIXME: Forward data to MQ
                        },
                        routing_key!(Auth >> Request) => {
                            // FIXME: Forward data to MQ
                        },
                        routing_key!(Consensus >> CompactSignedProposal) => {
                            // FIXME: Forward data to MQ
                        },
                        routing_key!(Synchronizer >> RawBytes) => {
                            // FIXME: Forward data to MQ
                        },
                        routing_key!(Auth >> GetBlockTxn) => {
                            // FIXME: Forward data to MQ
                        },
                        routing_key!(Synchronizer >> BlockTxn) => {
                            // FIXME: Forward data to MQ
                        },
                        _ => {
                            error!("Unexpected key {} from {:?}", key, source);
                        },
                    },
                }

            }
        }
    }

    fn snapshot_req(&self, data: &[u8]) {
        unimplemented!()
    }

    pub fn reply_rpc(&self, data: &[u8]) {
        let mut msg = Message::try_from(data).unwrap();

        let req_opt = msg.take_request();
        {
            if let Some(mut ts) = req_opt {
                let mut response = Response::new();
                response.set_request_id(ts.take_request_id());
                if ts.has_peercount() {
                    //FIXME: Get peer count, and send to Jsonrpc
                }
            } else {
                warn!("[reply_rpc] Receive unexpected rpc data");
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Source {
    LOCAL,
    REMOTE,
}