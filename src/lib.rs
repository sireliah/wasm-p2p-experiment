use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_channel::{bounded, Receiver, Sender};
use commands::TransferCommand;
use futures::{future::join, select, FutureExt, StreamExt};

use libp2p::{
    core::PeerRecord,
    identity::{self, Keypair},
    ping::PingEvent,
    rendezvous::{client::Event as RendezvousEvent, Cookie, Namespace},
    swarm::{AddressScore, NetworkBehaviourEventProcess, SwarmEvent},
    Multiaddr, PeerId,
};

use js_sys::Promise;
use peer::PeerEvent;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

pub mod behaviour;
pub mod commands;
pub mod file;
pub mod metadata;
pub mod peer;
pub mod protocol;
mod swarm;
mod utils;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/dragit.p2p.transfer.metadata.rs"));
}

use protocol::TransferPayload;
use swarm::{build_webrtc_swarm, build_ws_swarm, MyBehaviour, MyEvent};

#[macro_export]
macro_rules! console_log {
    // Note that this is using the `log` function imported above during
    // `bare_bones`
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

const NAMESPACE: &str = "discovery";
const SERVER_ADDR: &str = "127.0.0.1";

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[wasm_bindgen]
pub fn info(message: &str) {
    log(message);
}

fn discover(behaviour: &mut MyBehaviour, peer: PeerId, cookie: Cookie) {
    let namespace = Namespace::new(NAMESPACE.to_string()).expect("Failed to create namespace");
    behaviour
        .rendezvous
        .discover(Some(namespace), Some(cookie), None, peer);
}

impl NetworkBehaviourEventProcess<TransferPayload> for MyBehaviour {
    fn inject_event(&mut self, event: TransferPayload) {
        console_log!("Injected {}", event);
    }
}

// impl NetworkBehaviourEventProcess<TransferOut> for MyBehaviour {
//     fn inject_event(&mut self, event: TransferOut) {
//         console_log!("TransferOut event: {:?}", event);
//     }
// }

#[wasm_bindgen]
pub struct Server {
    rendezvous_addr: Multiaddr,
    local_keys: Keypair,
    // TODO: is this mutex really needed?
    known_peers: Arc<Mutex<HashMap<String, PeerRecord>>>,
    tx: Sender<PeerRecord>,
    rx: Arc<Receiver<PeerRecord>>,
    peer_sender: Sender<PeerEvent>,
    command_receiver: Receiver<TransferCommand>,
}

#[wasm_bindgen]
impl Server {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Server {
        let (peer_sender, peer_receiver) = bounded::<PeerEvent>(1024 * 24);
        let (command_sender, command_receiver) = bounded::<TransferCommand>(1024 * 24);

        let rendezvous_addr = format!("/ip4/{}/tcp/45555/ws", SERVER_ADDR)
            .parse::<Multiaddr>()
            .unwrap();
        let local_keys = identity::Keypair::generate_ed25519();

        let (tx, rx): (Sender<PeerRecord>, Receiver<PeerRecord>) = bounded(1024);
        Server {
            rendezvous_addr,
            local_keys,
            known_peers: Arc::new(Mutex::new(HashMap::new())),
            tx,
            // TODO: be careful with those references
            rx: Arc::new(rx),
            peer_sender,
            command_receiver,
        }
    }

    fn peer_id(&self) -> PeerId {
        PeerId::from(&self.local_keys.public())
    }

    #[wasm_bindgen]
    pub fn whoami(&self) -> String {
        self.peer_id().to_string()
    }

    #[wasm_bindgen]
    pub fn get_peers(&mut self) -> Promise {
        let peers = self
            .known_peers
            .lock()
            .unwrap()
            .iter()
            .map(|(_name, record)| record.peer_id().to_string())
            .collect::<Vec<String>>();
        let peers = peers.join(" ");
        let peers_result = Ok(JsValue::from(peers));
        future_to_promise(async { peers_result })
    }

    #[wasm_bindgen]
    pub fn call_peer(&mut self, peer_id: String) -> Promise {
        console_log!("Call peer: {:?}", peer_id);
        let response = match self.known_peers.lock().unwrap().get(&peer_id) {
            Some(record) => {
                self.tx.try_send(record.to_owned()).unwrap();
                Ok(JsValue::from_str("sent"))
            }
            None => Err(JsValue::from_str("")),
        };
        future_to_promise(async move { response })
    }

    // Why this function cannot be async?
    // https://github.com/rustwasm/wasm-bindgen/issues/1858
    #[wasm_bindgen]
    pub fn run_discovery(&mut self) {
        let addr = self.rendezvous_addr.clone();
        let webrtc_addr = format!("/ip4/{}/tcp/8080/ws/p2p-webrtc-star", SERVER_ADDR)
            .parse::<Multiaddr>()
            .unwrap();
        let command_rec = Arc::new(Mutex::new(self.command_receiver.to_owned()));
        let command_receiver_c = Arc::clone(&command_rec);

        let mut swarm = build_ws_swarm(self.local_keys.clone());
        let mut swarm_webrtc = build_webrtc_swarm(
            self.local_keys.clone(),
            self.peer_sender.clone(),
            command_receiver_c,
        );

        swarm_webrtc.listen_on(webrtc_addr).unwrap();

        swarm.add_external_address(self.rendezvous_addr.clone(), AddressScore::Infinite);

        swarm.dial(addr.clone()).expect("Dialing rendezvous failed");

        let known_peers = self.known_peers.clone();
        let local_peer_id = self.peer_id();
        let rx = self.rx.clone();

        let sw1 = async move {
            let mut cookie = None;
            while let Some(event) = swarm.next().await {
                match event {
                    SwarmEvent::NewListenAddr {
                        listener_id,
                        address,
                    } => {
                        console_log!("New listener: {:?}, Address: {:?}", listener_id, address);
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        console_log!("Connection established: {:?}", peer_id);
                        // TODO: check if this is rendezvous server
                        let behaviour = swarm.behaviour_mut();
                        let namespace = Namespace::new(NAMESPACE.to_string())
                            .expect("Failed to create namespace");
                        console_log!("Registering");
                        behaviour.rendezvous.register(namespace, peer_id, None);
                    }
                    SwarmEvent::ConnectionClosed {
                        cause,
                        endpoint,
                        num_established,
                        ..
                    } => {
                        console_log!(
                            "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
                        );
                        console_log!(
                            "Connection closed: {:?}, Endpoint: {:?}, Num established: {:?}",
                            cause,
                            endpoint,
                            num_established
                        );
                        console_log!(
                            "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
                        );

                        swarm.dial(addr.clone()).expect("Re-dialing failed")
                    }
                    SwarmEvent::Behaviour(MyEvent::Rendezvous(RendezvousEvent::Registered {
                        namespace,
                        rendezvous_node,
                        ..
                    })) => {
                        console_log!("Registered in {:?}", namespace);
                        console_log!("Discovering");
                        let behaviour = swarm.behaviour_mut();
                        behaviour
                            .rendezvous
                            .discover(Some(namespace), None, None, rendezvous_node);
                    }
                    SwarmEvent::Behaviour(MyEvent::Rendezvous(RendezvousEvent::Discovered {
                        registrations,
                        cookie: new_cookie,
                        ..
                    })) => {
                        // console_log!("Discovered some peers!");
                        cookie.replace(new_cookie);
                        for registration in registrations {
                            if registration.record.peer_id() != local_peer_id {
                                known_peers
                                    .lock()
                                    .expect("Inserting on lock failed")
                                    .insert(
                                        registration.record.peer_id().to_string(),
                                        registration.record,
                                    );
                            }
                        }
                    }
                    SwarmEvent::Behaviour(MyEvent::Ping(PingEvent { result, peer })) => {
                        // console_log!("Discovery: Ping: {:?}", result);
                        if let Some(ref some_cookie) = cookie {
                            let behaviour = swarm.behaviour_mut();
                            discover(behaviour, peer, some_cookie.clone());
                        }
                    }
                    other => console_log!("Event: {:?}", other),
                }
            }
            swarm
        };

        let sw2 = async move {
            loop {
                select! {
                    event = swarm_webrtc.next().fuse() => {
                        match event {
                            Some(SwarmEvent::NewListenAddr { address, .. }) => {
                                console_log!("Webrtc: new listener: {:?}", address);
                            }
                            Some(SwarmEvent::ConnectionEstablished { peer_id, .. }) => {
                                console_log!("Webrtc: established to {:?}", peer_id);
                            }
                            Some(other) => {
                                console_log!("Webrtc: other: {:?}", other);
                            }
                            None => console_log!("Nothing yet"),
                        };
                    }
                    record = rx.recv().fuse() => {
                        match record {
                                Ok(rec) => {
                                    console_log!("Record!!! {:?}", rec);
                                    let address = format!(
                                        "/ip4/{}/tcp/8080/ws/p2p-webrtc-star/p2p/{}",
                                        SERVER_ADDR,
                                        rec.peer_id()
                                    )
                                    .parse::<Multiaddr>()
                                    .expect("Parsing failed");
                                    if !swarm_webrtc.is_connected(&rec.peer_id()) {
                                        swarm_webrtc.dial(address).expect("Dialing webrtc failed");
                                    }
                                }
                                Err(e) => {
                                    console_log!("Recv error: {:?}", e);
                                }
                            };
                        }
                }
            }
        };

        future_to_promise(async move {
            let tuple = join(sw1, sw2);

            tuple.await;
            Ok(JsValue::from(true))
        });
    }
}
