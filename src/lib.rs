mod utils;

use libp2p::{PeerId, identity};
// use libp2p_webrtc::WebRtcTransport;

use wasm_bindgen::prelude::*;


// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

macro_rules! console_log {
    // Note that this is using the `log` function imported above during
    // `bare_bones`
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}


fn build_swarm() {
    let local_keys = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_keys.public());
    console_log!("I am peer {:?}", local_peer_id);

    // let transport = {
    //     let base = WebRtcTransport::new(local_peer_id, vec!["stun:stun.l.google.com:19302"]);
    //     let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
    //         .into_authentic(&local_keys)
    //         .expect("Signing libp2p-noise static DH keypair failed.");


    //     let mut mplex_config = mplex::MplexConfig::new();
    //     let mp = mplex_config
    //         .set_max_buffer_size(40960)
    //         .set_split_send_size(1024 * 512);
    //     let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
    //         .into_authentic(&local_keys)
    //         .unwrap();
    //     let noise = noise::NoiseConfig::xx(noise_keys).into_authenticated();

    //     base
    //         .upgrade(upgrade::Version::V1)
    //         .authenticate(noise)
    //         .multiplex(mp.clone())
    //         .timeout(std::time::Duration::from_secs(20))
    //         .boxed()


    //     // base.upgrade()
    //     //     .authenticate_with_version(
    //     //         noise::NoiseConfig::xx(noise_keys).into_authenticated(),
    //     //         core::upgrade::AuthenticationVersion::V1SimultaneousOpen,
    //     //     )
    //     //     .multiplex(core::upgrade::SelectUpgrade::new(
    //     //         yamux::YamuxConfig::default(),
    //     //         mplex::MplexConfig::default(),
    //     //     ))
    //     //     .timeout(std::time::Duration::from_secs(20))
    //     //     .boxed()
    // };
}


#[wasm_bindgen]
pub fn info(message: &str) {
    log(message);
}

#[wasm_bindgen]
pub fn discover() {}


#[wasm_bindgen]
pub fn listen() {
    utils::set_panic_hook();
    build_swarm();
    // console_log!("Peer {:?}", peer);
}