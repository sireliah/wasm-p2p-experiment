use libp2p::{
    core::transport::upgrade,
    core::PeerRecord,
    futures::future::join,
    identity::{self, Keypair},
    mplex, noise,
    ping::{Ping, PingConfig, PingEvent},
    rendezvous::{
        client::Behaviour as RendezvousBehaviour, client::Event as RendezvousEvent, Namespace,
    },
    swarm::{AddressScore, SwarmBuilder, SwarmEvent},
    Multiaddr, NetworkBehaviour, PeerId, Swarm, Transport,
};

use futures;
use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::{SinkExt, StreamExt};
use libp2p::wasm_ext;

use js_sys::Promise;
use libp2p_webrtc::WebRtcTransport;
use std::cell::RefCell;
use std::rc::Rc;
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

fn discover(behaviour: &mut MyBehaviour, peer: PeerId) {
    let namespace = Namespace::new("discovery".to_string()).expect("Failed to create namespace");
    behaviour
        .rendezvous
        .discover(Some(namespace), None, None, peer);
}

#[wasm_bindgen]
pub struct Server {
    rendezvous_addr: Multiaddr,
    local_keys: Keypair,
    tx: Sender<PeerRecord>,
    rx: Rc<RefCell<Receiver<PeerRecord>>>,
}

#[wasm_bindgen]
impl Server {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Server {
        let rendezvous_addr = "/ip4/127.0.0.1/tcp/45555/ws".parse::<Multiaddr>().unwrap();
        let local_keys = identity::Keypair::generate_ed25519();
        let (tx, rx): (Sender<PeerRecord>, Receiver<PeerRecord>) = channel(100);

        let receiver = Rc::new(RefCell::new(rx));
        Server {
            rendezvous_addr,
            local_keys,
            tx,
            rx: receiver,
        }
    }

    // Why this function cannot be async?
    // https://github.com/rustwasm/wasm-bindgen/issues/1858
    #[wasm_bindgen]
    pub fn discover_peers(&mut self, peer_address: String) {
        let mut swarm = build_ws_swarm(self.local_keys.clone());
        swarm.add_external_address(self.rendezvous_addr.clone(), AddressScore::Infinite);

        swarm
            .dial(self.rendezvous_addr.clone())
            .expect("Dialing rendezvous failed");

        let mut tx = self.tx.clone();
        future_to_promise(async move {
            while let Some(event) = swarm.next().await {
                match event {
                    SwarmEvent::NewListenAddr {
                        listener_id,
                        address,
                    } => {
                        console_log!("New listener: {:?}, Address: {:?}", listener_id, address);
                        if peer_address.len() > 0 {
                            let addr = peer_address
                                .parse::<Multiaddr>()
                                .expect("Parsing address failed");
                            swarm.dial(addr).expect("Dialing failed");
                        }
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        console_log!("Connection established: {:?}", peer_id);
                        // TODO: check if this is rendezvous server
                        let behaviour = swarm.behaviour_mut();
                        let namespace = Namespace::new("discovery".to_string())
                            .expect("Failed to create namespace");
                        let namespace_c = namespace.clone();
                        console_log!("Registering");
                        behaviour.rendezvous.register(namespace, peer_id, None);

                        console_log!("Discovering");
                        behaviour
                            .rendezvous
                            .discover(Some(namespace_c), None, None, peer_id);
                    }
                    SwarmEvent::Behaviour(MyEvent::Rendezvous(RendezvousEvent::Registered {
                        namespace,
                        ..
                    })) => {
                        console_log!("Registered in {:?}", namespace);
                    }
                    SwarmEvent::Behaviour(MyEvent::Rendezvous(RendezvousEvent::Discovered {
                        registrations,
                        ..
                    })) => {
                        console_log!("Discovered some peers!");
                        for registration in registrations {
                            console_log!(
                                "  Peer: {:?}, Addresses: {:?}",
                                registration.record.addresses(),
                                registration.record.peer_id()
                            );
                            tx.send(registration.record)
                                .await
                                .expect("Sending record failed");
                        }
                    }
                    SwarmEvent::Behaviour(MyEvent::Ping(PingEvent { result, peer })) => {
                        console_log!("Ping {:?}", result);
                        let behaviour = swarm.behaviour_mut();
                        discover(behaviour, peer);
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

#[wasm_bindgen]
pub async fn listen(peer_address: String) {
    utils::set_panic_hook();
    let local_keys = identity::Keypair::generate_ed25519();
    let local_keys_clone = local_keys.clone();
    let webrtc_addr = "/ip4/127.0.0.1/tcp/8000/ws/p2p-webrtc-star"
        .parse::<Multiaddr>()
        .unwrap();
    let rendezvous_addr = "/ip4/127.0.0.1/tcp/45555/ws".parse::<Multiaddr>().unwrap();

    let mut swarm = build_ws_swarm(local_keys);
    let mut webrtc_swarm = build_webrtc_swarm(local_keys_clone);

    // "/dns4/wrtc-star1.par.dwebops.pub/tcp/443/wss/p2p-webrtc-star"
    // let address = "/dns4/127.0.0.1/tcp/8000/ws/p2p-webrtc-star"
    //     .parse()
    //     .unwrap();
    swarm.add_external_address(rendezvous_addr.clone(), AddressScore::Infinite);

    // TODO: this might fill up pretty quickly
    let (mut tx, rx) = channel(100);

    webrtc_swarm.listen_on(webrtc_addr).unwrap();

    swarm
        .dial(rendezvous_addr)
        .expect("Dialing rendezvous failed");
    let sw1 = async move {
        while let Some(event) = swarm.next().await {
            match event {
                SwarmEvent::NewListenAddr {
                    listener_id,
                    address,
                } => {
                    console_log!("New listener: {:?}, Address: {:?}", listener_id, address);
                    if peer_address.len() > 0 {
                        let addr = peer_address
                            .parse::<Multiaddr>()
                            .expect("Parsing address failed");
                        swarm.dial(addr).expect("Dialing failed");
                    }
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    console_log!("Connection established: {:?}", peer_id);
                    // TODO: check if this is rendezvous server
                    let behaviour = swarm.behaviour_mut();
                    let namespace = Namespace::new("discovery".to_string())
                        .expect("Failed to create namespace");
                    let namespace_c = namespace.clone();
                    console_log!("Registering");
                    behaviour.rendezvous.register(namespace, peer_id, None);

                    console_log!("Discovering");
                    behaviour
                        .rendezvous
                        .discover(Some(namespace_c), None, None, peer_id);
                }
                SwarmEvent::Behaviour(MyEvent::Rendezvous(RendezvousEvent::Registered {
                    namespace,
                    ..
                })) => {
                    console_log!("Registered in {:?}", namespace);
                }
                SwarmEvent::Behaviour(MyEvent::Rendezvous(RendezvousEvent::Discovered {
                    registrations,
                    ..
                })) => {
                    console_log!("Discovered some peers!");
                    for registration in registrations {
                        console_log!(
                            "  Peer: {:?}, Addresses: {:?}",
                            registration.record.addresses(),
                            registration.record.peer_id()
                        );
                        tx.send(registration.record)
                            .await
                            .expect("Sending record failed");
                    }
                }
                SwarmEvent::Behaviour(MyEvent::Ping(PingEvent { result, peer })) => {
                    console_log!("Ping {:?}", result);
                    let behaviour = swarm.behaviour_mut();
                    discover(behaviour, peer);
                }
                other => console_log!("Event: {:?}", other),
            }
        }
        swarm
    };
    let sw2 = async move {
        while let Some(event) = webrtc_swarm.next().await {
            match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    console_log!("Webrtc: new listener: {:?}", address);
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    console_log!("Webrtc: established to {:?}", peer_id);
                }
                other => {
                    console_log!("Webrtc: other: {:?}", other);
                }
            };
        }
        webrtc_swarm
    };
    futures::join!(sw1, sw2);
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
            .timeout(std::time::Duration::from_secs(20))
            .boxed()
    };

    let behaviour = MyBehaviour {
        rendezvous: RendezvousBehaviour::new(local_keys),
        ping: Ping::new(PingConfig::new().with_interval(Duration::from_secs(2))),
    };

    let swarm = SwarmBuilder::new(transport, behaviour, local_peer_id);
    swarm.build()
}

#[allow(dead_code)]
fn build_webrtc_swarm(local_keys: identity::Keypair) -> Swarm<Ping> {
    let local_peer_id = PeerId::from(&local_keys.public());

    let transport_base = WebRtcTransport::new(local_peer_id, vec!["stun:stun.l.google.com:19302"]);
    let transport = {
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
            .timeout(std::time::Duration::from_secs(20))
            .boxed()
    };

    let ping_behaviour = Ping::new(
        PingConfig::new()
            .with_interval(Duration::from_secs(2))
            .with_keep_alive(true),
    );
    let swarm = SwarmBuilder::new(transport, ping_behaviour, local_peer_id);
    swarm.executor(Box::new(|f| spawn_local(f))).build()
}
