pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

// let sw2 = async move {
//     while let Some(event) = webrtc_swarm.next().await {
//         match event {
//             SwarmEvent::NewListenAddr { address, .. } => {
//                 console_log!("Webrtc: new listener: {:?}", address);
//             }
//             SwarmEvent::ConnectionEstablished { peer_id, .. } => {
//                 console_log!("Webrtc: established to {:?}", peer_id);
//             }
//             other => {
//                 console_log!("Webrtc: other: {:?}", other);
//             }
//         };
//     }
//     webrtc_swarm
// };
