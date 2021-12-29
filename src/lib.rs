use libp2p::{
    core::transport::upgrade,
    core::PeerRecord,
    futures::future::join,
    identity::{self, Keypair},
    mplex, noise,
    ping::{Ping, PingConfig, PingEvent},
    rendezvous::{
        client::Behaviour as RendezvousBehaviour, client::Event as RendezvousEvent, Cookie,
        Namespace, Registration,
    },
    swarm::{AddressScore, SwarmBuilder, SwarmEvent},
    Multiaddr, NetworkBehaviour, PeerId, Swarm, Transport,
};

use futures;
use futures::{SinkExt, StreamExt};
use libp2p::wasm_ext;

use js_sys::Promise;
// use libp2p_webrtc::WebRtcTransport;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, spawn_local};

mod utils;

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

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[derive(Debug)]
enum MyEvent {
    Ping(PingEvent),
    Rendezvous(RendezvousEvent),
}

impl From<PingEvent> for MyEvent {
    fn from(event: PingEvent) -> Self {
        MyEvent::Ping(event)
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MyEvent")]
struct MyBehaviour {
    rendezvous: RendezvousBehaviour,
    ping: Ping,
}

impl From<RendezvousEvent> for MyEvent {
    fn from(event: RendezvousEvent) -> Self {
        MyEvent::Rendezvous(event)
    }
}

#[wasm_bindgen]
pub fn info(message: &str) {
    log(message);
}

fn discover(behaviour: &mut MyBehaviour, peer: PeerId, cookie: Cookie) {
    let namespace = Namespace::new(NAMESPACE.to_string()).expect("Failed to create namespace");
    behaviour
        .rendezvous
        .discover(Some(namespace), None, None, peer);
}

#[wasm_bindgen]
pub struct Server {
    rendezvous_addr: Multiaddr,
    local_keys: Keypair,
    known_peers: Arc<Mutex<HashMap<String, PeerRecord>>>,
}

#[wasm_bindgen]
impl Server {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Server {
        let rendezvous_addr = "/ip4/127.0.0.1/tcp/45555/ws".parse::<Multiaddr>().unwrap();
        let local_keys = identity::Keypair::generate_ed25519();

        Server {
            rendezvous_addr,
            local_keys,
            known_peers: Arc::new(Mutex::new(HashMap::new())),
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
            .map(|(name, record)| record.peer_id().to_string())
            .collect::<Vec<String>>();
        let peers = peers.join(" ");
        let peers_result = Ok(JsValue::from(peers));
        future_to_promise(async { peers_result })
    }

    // Why this function cannot be async?
    // https://github.com/rustwasm/wasm-bindgen/issues/1858
    #[wasm_bindgen]
    pub fn run_discovery(&mut self) {
        let addr = self.rendezvous_addr.clone();
        let mut swarm = build_ws_swarm(self.local_keys.clone());
        swarm.add_external_address(self.rendezvous_addr.clone(), AddressScore::Infinite);

        swarm.dial(addr.clone()).expect("Dialing rendezvous failed");

        let known_peers = self.known_peers.clone();
        let local_peer_id = self.peer_id();
        future_to_promise(async move {
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
                        peer_id,
                        cause,
                        endpoint,
                        num_established,
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
                        console_log!("Discovered some peers!");
                        cookie.replace(new_cookie);
                        for registration in registrations {
                            console_log!(
                                "  Peer: {:?}, Addresses: {:?}",
                                registration.record.addresses(),
                                registration.record.peer_id()
                            );
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
                        console_log!("Ping: {:?}", result);
                        if let Some(ref some_cookie) = cookie {
                            console_log!("Will try to discover: {:?}", some_cookie);
                            let behaviour = swarm.behaviour_mut();
                            discover(behaviour, peer, some_cookie.clone());
                        }
                    }
                    other => console_log!("Event: {:?}", other),
                }
            }
            let sw = async { swarm };
            sw.await;
            Ok(JsValue::from(true))
        });
    }
}

fn build_ws_swarm(local_keys: identity::Keypair) -> Swarm<MyBehaviour> {
    let local_peer_id = PeerId::from(&local_keys.public());

    console_log!("I am peer {:?}", local_peer_id);

    let transport = {
        let transport_base = wasm_ext::ffi::websocket_transport();
        let transport_base = wasm_ext::ExtTransport::new(transport_base);
        let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&local_keys)
            .expect("Failed to produce noise keys");
        let mut mplex_config = mplex::MplexConfig::new();

        let mp = mplex_config
            .set_max_buffer_size(40960)
            .set_split_send_size(1024 * 512);
        let noise = noise::NoiseConfig::xx(noise_keys).into_authenticated();
        transport_base
            .upgrade(upgrade::Version::V1Lazy)
            .authenticate(noise)
            .multiplex(mp.clone())
            .timeout(std::time::Duration::from_secs(20000))
            .boxed()
    };

    let behaviour = MyBehaviour {
        rendezvous: RendezvousBehaviour::new(local_keys),
        ping: Ping::new(
            PingConfig::new()
                .with_timeout(Duration::from_secs(2000))
                .with_interval(Duration::from_secs(1))
                .with_max_failures(NonZeroU32::new(100).unwrap()),
        ),
    };

    let swarm = SwarmBuilder::new(transport, behaviour, local_peer_id);
    swarm.build()
}

// #[allow(dead_code)]
// fn build_webrtc_swarm(local_keys: identity::Keypair) -> Swarm<Ping> {
//     let local_peer_id = PeerId::from(&local_keys.public());

//     let transport_base = WebRtcTransport::new(local_peer_id, vec!["stun:stun.l.google.com:19302"]);
//     let transport = {
//         let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
//             .into_authentic(&local_keys)
//             .expect("Failed to produce noise keys");
//         let mut mplex_config = mplex::MplexConfig::new();

//         let mp = mplex_config
//             .set_max_buffer_size(40960)
//             .set_split_send_size(1024 * 512);
//         let noise = noise::NoiseConfig::xx(noise_keys).into_authenticated();
//         transport_base
//             .upgrade(upgrade::Version::V1Lazy)
//             .authenticate(noise)
//             .multiplex(mp.clone())
//             .timeout(std::time::Duration::from_secs(20))
//             .boxed()
//     };

//     let ping_behaviour = Ping::new(
//         PingConfig::new()
//             .with_interval(Duration::from_secs(2))
//             .with_keep_alive(true),
//     );
//     let swarm = SwarmBuilder::new(transport, ping_behaviour, local_peer_id);
//     swarm.executor(Box::new(|f| spawn_local(f))).build()
// }
