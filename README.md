# Hello wasm proof-of-concept project

Try to see if [Dragit](https://github.com/sireliah/dragit) can be running in a browser.

1. try to run peer discovery using Rendezvous protocol
2. take discovered peers and use [libp2p WebRTC](https://github.com/wngr/libp2p-webrtc) to initiate connection between them
3. finally, use [Dragit](https://github.com/sireliah/dragit) protocol to transfer files between peers

**Watch out**: this is experimental project. Expect dragons here.

## TODO:
- discovery by namespaces: preferably rendezvous server side
- webrtc dialing
- dragit protocol on webrtc


## Resources
- FileReader: https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.FileReader.html#method.onload
- Fiddle for FileReader chunks: http://jsfiddle.net/mw99v8d4/
- ArrayBuffer processing large files: https://joji.me/en-us/blog/processing-huge-files-using-filereader-readasarraybuffer-in-web-browser/
- Mozilla file handling docs, very useful: https://developer.mozilla.org/en-US/docs/Web/API/File/Using_files_from_web_applications
- libp2p application on wasm, great example: https://github.com/iotaledger/identity.rs/blob/301193625bb4133c63b7af5b7f1d0b5b0423b9fb/bindings/wasm/src/actor/mod.rs#L51-L53
