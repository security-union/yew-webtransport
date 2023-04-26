# yew-webtransport

## Summary

Access WebTransport in Yew apps using wasm-bindgen https://github.com/rustwasm/wasm-bindgen/pull/3344

YouTube Video: https://youtu.be/dztIToTf8Yc

![thumbnail7](https://user-images.githubusercontent.com/1176339/224917256-68ae5fff-dc1c-4f29-8656-ae9232634cd7.png)

## Justification

WebTransport is a new web standard that aims to provide a low-level API for secure, multiplexed communication between web browsers and servers. It has the potential to greatly improve the performance of web applications, especially those that require real-time communication or large data transfers. However, the current implementation of WebTransport in browsers is written in Javascript, which can make it difficult to use in Rust webapps.

## Demo

you can find a demo in the examples folder which is a port of the https://webtransport.day/ website to Yew + Rust.

The website is live at https://security-union.github.io/yew-webtransport/

![send datagram](https://user-images.githubusercontent.com/1176339/224579691-6d8c1451-a935-4d75-a4a0-556305195c36.gif)

If you want to run it locally, you have to set RUSTFLAGS

Until wasm-bindgen releases my changes to crates.io you will have to install wasm-bindgen-cli by hand:
```
cargo install -f wasm-bindgen-cli --git https://github.com/rustwasm/wasm-bindgen.git  --rev 27173549f6a196a869cec525f8f87adec55c738c
```
cd examples/webtransport-demo
 WS_URL=https://127.0.0.1:4433 RUSTFLAGS=--cfg=web_sys_unstable_apis trunk serve 
```
