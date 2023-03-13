# yew-webtransport

## Summary

Access WebTransport in Yew apps using wasm-bindgen https://github.com/rustwasm/wasm-bindgen/pull/3344

## Justification

WebTransport is a new web standard that aims to provide a low-level API for secure, multiplexed communication between web browsers and servers. It has the potential to greatly improve the performance of web applications, especially those that require real-time communication or large data transfers. However, the current implementation of WebTransport in browsers is written in Javascript, which can make it difficult to use in Rust webapps.

## Demo

you can find a demo in the examples folder which is a port of the https://webtransport.day/ website to Yew + Rust.

The website is live at https://security-union.github.io/yew-webtransport/

![send datagram](https://user-images.githubusercontent.com/1176339/224579691-6d8c1451-a935-4d75-a4a0-556305195c36.gif)

