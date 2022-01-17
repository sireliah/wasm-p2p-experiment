use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_channel::{Receiver, Sender};
use libp2p::{
    core::transport::upgrade,
    identity::Keypair,
    mplex, noise,
    ping::{Ping, PingConfig, PingEvent},
    rendezvous::{client::Behaviour as RendezvousBehaviour, client::Event as RendezvousEvent},
    swarm::SwarmBuilder,
    wasm_ext, NetworkBehaviour, PeerId, Swarm, Transport,
};
use libp2p_webrtc::WebRtcTransport;
use wasm_bindgen_futures::spawn_local;

use crate::behaviour::TransferBehaviour;
use crate::console_log;
use crate::protocol::{TransferOut, TransferPayload};
use crate::{behaviour::TransferEvent, commands::TransferCommand, log, peer::PeerEvent};

#[derive(Debug)]
pub enum MyEvent {
    // TransferBehaviour(TransferEvent),
    Rendezvous(RendezvousEvent),
    Ping(PingEvent),
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MyEvent")]
pub struct MyBehaviour {
    pub rendezvous: RendezvousBehaviour,
    ping: Ping,
    // transfer: TransferBehaviour,
}

impl From<RendezvousEvent> for MyEvent {
    fn from(event: RendezvousEvent) -> Self {
        MyEvent::Rendezvous(event)
    }
}

// impl From<TransferEvent> for MyEvent {
//     fn from(event: TransferEvent) -> Self {
//         MyEvent::TransferBehaviour(event)
//     }
// }

impl From<PingEvent> for MyEvent {
    fn from(event: PingEvent) -> Self {
        MyEvent::Ping(event)
    }
}

pub fn build_ws_swarm(local_keys: Keypair) -> Swarm<MyBehaviour> {
    let local_peer_id = PeerId::from(&local_keys.public());

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

pub fn build_webrtc_swarm(
    local_keys: Keypair,
    peer_sender: Sender<PeerEvent>,
    command_receiver: Arc<Mutex<Receiver<TransferCommand>>>,
) -> Swarm<TransferBehaviour> {
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
            // Keep connection for longer now
            .timeout(std::time::Duration::from_secs(200))
            .boxed()
    };

    // let ping_behaviour = Ping::new(
    //     PingConfig::new()
    //         .with_interval(Duration::from_secs(2))
    //         .with_keep_alive(true),
    // );

    let transfer_behaviour = TransferBehaviour::new(peer_sender.clone(), command_receiver, None);

    let swarm = SwarmBuilder::new(transport, transfer_behaviour, local_peer_id);
    swarm.executor(Box::new(|f| spawn_local(f))).build()
}
