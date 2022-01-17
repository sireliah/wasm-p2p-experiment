use std::error::Error;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use async_channel::{Receiver, Sender};

use libp2p::core::{connection::ConnectionId, ConnectedPoint, Multiaddr, PeerId};
use libp2p::swarm::{
    NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, OneShotHandler, OneShotHandlerConfig,
    PollParameters, SubstreamProtocol,
};

use crate::commands::TransferCommand;
use crate::console_log;
use crate::file::{FileToSend, Payload};
use crate::log;
use crate::peer::PeerEvent;
use crate::protocol::{ProtocolEvent, TransferOut, TransferPayload};

const TIMEOUT: u64 = 600;

#[derive(Debug)]
pub struct TransferEvent(TransferPayload);

pub struct TransferBehaviour {
    pub events: Vec<
        NetworkBehaviourAction<
            TransferEvent,
            OneShotHandler<TransferPayload, TransferOut, ProtocolEvent>,
        >,
    >,
    payloads: Vec<FileToSend>,
    pub sender: Sender<PeerEvent>,
    receiver: Arc<Mutex<Receiver<TransferCommand>>>,
    pub target_path: Option<String>,
}

impl TransferBehaviour {
    pub fn new(
        sender: Sender<PeerEvent>,
        receiver: Arc<Mutex<Receiver<TransferCommand>>>,
        target_path: Option<String>,
    ) -> Self {
        TransferBehaviour {
            events: vec![],
            payloads: vec![],
            sender,
            receiver,
            target_path,
        }
    }

    pub fn push_file(&mut self, file: FileToSend) -> Result<(), Box<dyn Error>> {
        Ok(self.payloads.push(file))
    }
}

impl NetworkBehaviour for TransferBehaviour {
    type ProtocolsHandler = OneShotHandler<TransferPayload, TransferOut, ProtocolEvent>;
    type OutEvent = TransferEvent;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        let timeout = Duration::from_secs(TIMEOUT);
        let tp = TransferPayload {
            name: "default".to_string(),
            hash: "".to_string(),
            payload: Payload::Path(".".to_string()),
            size_bytes: 0,
            sender_queue: self.sender.clone(),
            receiver: Arc::clone(&self.receiver),
            target_path: self.target_path.clone(),
        };
        let handler_config = OneShotHandlerConfig {
            keep_alive_timeout: Duration::from_secs(5000),
            outbound_substream_timeout: timeout,
            // Default from the library
            max_dial_negotiated: 8,
        };
        let proto = SubstreamProtocol::new(tp, ()).with_timeout(timeout);
        Self::ProtocolsHandler::new(proto, handler_config)
    }

    fn addresses_of_peer(&mut self, _peer_id: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connected(&mut self, _peer: &PeerId) {}

    fn inject_connection_established(
        &mut self,
        peer: &PeerId,
        c: &ConnectionId,
        endpoint: &ConnectedPoint,
        _failed_addresses: Option<&Vec<Multiaddr>>,
    ) {
        console_log!(
            "Connection established: {:?}, {:?}, c: {:?}",
            peer,
            endpoint,
            c
        )
    }

    // fn inject_dial_failure(&mut self, peer: &PeerId) {
    //     console_log!("Dial failure: {:?}", peer);
    // }

    fn inject_disconnected(&mut self, _peer: &PeerId) {}

    fn inject_event(&mut self, _: PeerId, _: ConnectionId, event: ProtocolEvent) {
        console_log!("Inject event: {}", event);
        match event {
            ProtocolEvent::Received(data) => self
                .events
                .push(NetworkBehaviourAction::GenerateEvent(TransferEvent(data))),
            ProtocolEvent::Sent => return,
        };
    }

    fn poll(
        &mut self,
        _: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ProtocolsHandler>> {
        if let Some(file) = self.payloads.pop() {
            console_log!("File in the queue: {:?}", file);
            let peer_id = file.peer.clone();
            let transfer = TransferOut {
                file,
                sender_queue: self.sender.clone(),
            };

            let event = NetworkBehaviourAction::NotifyHandler {
                // TODO: Notify particular handler, not Any
                handler: NotifyHandler::Any,
                peer_id,
                event: transfer,
            };
            self.events.push(event);
        }

        match self.events.pop() {
            Some(event) => Poll::Ready(event),
            None => Poll::Pending,
        }
    }
}
