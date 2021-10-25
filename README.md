# Hello wasm proof-of-concept project

Try to see if [Dragit](https://github.com/sireliah/dragit) can be running in a browser.

1. try to run peer discovery using Rendezvous protocol
2. take discovered peers and use [libp2p WebRTC](https://github.com/wngr/libp2p-webrtc) to initiate connection between them
3. finally, use [Dragit](https://github.com/sireliah/dragit) protocol to transfer files between peers

**Watch out**: this is experimental project. Expect dragons here.