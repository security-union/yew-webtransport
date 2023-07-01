//! A service to connect to a server through the
//! [`WebTransport` Protocol](https://datatracker.ietf.org/doc/draft-ietf-webtrans-overview/).

/**
MIT License

Copyright (c) 2022 Security Union

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
 */
use anyhow::{anyhow, Error};
use std::{fmt, rc::Rc};
use thiserror::Error as ThisError;
use wasm_bindgen_futures::JsFuture;
use yew::callback::Callback;
use yew::platform::pinned::oneshot::channel;

use gloo_console::log;
use js_sys::{Boolean, JsString, Promise, Reflect, Uint8Array};
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::{
    ReadableStream, ReadableStreamDefaultReader, WebTransport, WebTransportBidirectionalStream,
    WebTransportCloseInfo, WebTransportDatagramDuplexStream, WebTransportReceiveStream,
    WritableStream,
};

/// Represents formatting errors.
#[derive(Debug, ThisError)]
pub enum FormatError {
    /// Received text for a binary format, e.g. someone sending text
    /// on a WebTransport that is using a binary serialization format, like Cbor.
    #[error("received text for a binary format")]
    ReceivedTextForBinary,
    /// Received binary for a text format, e.g. someone sending binary
    /// on a WebTransport that is using a text serialization format, like Json.
    #[error("received binary for a text format")]
    ReceivedBinaryForText,
    /// Trying to encode a binary format as text", e.g., trying to
    /// store a Cbor encoded value in a String.
    #[error("trying to encode a binary format as Text")]
    CantEncodeBinaryAsText,
}

/// A representation of a value which can be stored and restored as a text.
///
/// Some formats are binary only and can't be serialized to or deserialized
/// from Text.  Attempting to do so will return an Err(FormatError).
pub type Text = Result<String, Error>;

/// A representation of a value which can be stored and restored as a binary.
pub type Binary = Result<Vec<u8>, Error>;

/// The status of a WebTransport connection. Used for status notifications.
#[derive(Clone, Debug, PartialEq)]
pub enum WebTransportStatus {
    /// Fired when a WebTransport connection has opened.
    Opened,
    /// Fired when a WebTransport connection has closed.
    Closed(JsValue),
    /// Fired when a WebTransport connection has failed.
    Error(JsValue),
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
/// An error encountered by a WebTransport.
pub enum WebTransportError {
    #[error("{0}")]
    /// An error encountered when creating the WebTransport.
    CreationError(String),
}

/// A handle to control the WebTransport connection. Implements `Task` and could be canceled.
#[must_use = "the connection will be closed when the task is dropped"]
pub struct WebTransportTask {
    pub transport: Rc<WebTransport>,
    #[allow(dead_code)]
    notification: Callback<WebTransportStatus>,
    #[allow(dead_code)]
    listeners: [Promise; 2],
}

impl WebTransportTask {
    fn new(
        transport: Rc<WebTransport>,
        notification: Callback<WebTransportStatus>,
        listeners: [Promise; 2],
    ) -> WebTransportTask {
        WebTransportTask {
            transport,
            notification,
            listeners,
        }
    }
}

impl fmt::Debug for WebTransportTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WebTransportTask")
    }
}

/// A WebTransport service attached to a user context.
#[derive(Default, Debug)]
pub struct WebTransportService {}

impl WebTransportService {
    /// Connects to a server through a WebTransport connection. Needs two callbacks; one is passed
    /// data, the other is passed updates about the WebTransport's status.
    pub fn connect(
        url: &str,
        on_datagram: Callback<Vec<u8>>,
        on_unidirectional_stream: Callback<WebTransportReceiveStream>,
        on_bidirectional_stream: Callback<WebTransportBidirectionalStream>,
        notification: Callback<WebTransportStatus>,
    ) -> Result<WebTransportTask, WebTransportError> {
        let ConnectCommon(transport, listeners) = Self::connect_common(url, &notification)?;
        let transport = Rc::new(transport);

        Self::start_listening_incoming_datagrams(
            transport.clone(),
            transport.datagrams(),
            on_datagram,
        );
        Self::start_listening_incoming_unidirectional_streams(
            transport.clone(),
            transport.incoming_unidirectional_streams(),
            on_unidirectional_stream,
        );

        Self::start_listening_incoming_bidirectional_streams(
            transport.clone(),
            transport.incoming_bidirectional_streams(),
            on_bidirectional_stream,
        );

        Ok(WebTransportTask::new(transport, notification, listeners))
    }

    fn start_listening_incoming_unidirectional_streams(
        transport: Rc<WebTransport>,
        incoming_streams: ReadableStream,
        callback: Callback<WebTransportReceiveStream>,
    ) {
        let read_result: ReadableStreamDefaultReader =
            incoming_streams.get_reader().unchecked_into();
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let read_result = JsFuture::from(read_result.read()).await;
                match read_result {
                    Err(e) => {
                        log!("Failed to read incoming unidirectional streams", &e);
                        let mut reason = WebTransportCloseInfo::default();
                        reason.reason(
                            format!("Failed to read incoming unidirectional streams {e:?}")
                                .as_str(),
                        );
                        transport.close_with_close_info(&reason);
                        break;
                    }
                    Ok(result) => {
                        let done = Reflect::get(&result, &JsString::from("done"))
                            .unwrap()
                            .unchecked_into::<Boolean>();
                        if let Ok(value) = Reflect::get(&result, &JsString::from("value")) {
                            if value.is_undefined() {
                                break;
                            }
                            let value: WebTransportReceiveStream = value.unchecked_into();
                            callback.emit(value);
                        }
                        if done.is_truthy() {
                            break;
                        }
                    }
                }
            }
        });
    }

    fn start_listening_incoming_datagrams(
        transport: Rc<WebTransport>,
        datagrams: WebTransportDatagramDuplexStream,
        callback: Callback<Vec<u8>>,
    ) {
        let incoming_datagrams: ReadableStreamDefaultReader =
            datagrams.readable().get_reader().unchecked_into();
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let read_result = JsFuture::from(incoming_datagrams.read()).await;
                match read_result {
                    Err(e) => {
                        let mut reason = WebTransportCloseInfo::default();
                        reason.reason(format!("Failed to read incoming datagrams {e:?}").as_str());
                        transport.close_with_close_info(&reason);
                        break;
                    }
                    Ok(result) => {
                        let done = Reflect::get(&result, &JsString::from("done"))
                            .unwrap()
                            .unchecked_into::<Boolean>();
                        if done.is_truthy() {
                            break;
                        }
                        let value: Uint8Array = Reflect::get(&result, &JsString::from("value"))
                            .unwrap()
                            .unchecked_into();
                        process_binary(&value, &callback);
                    }
                }
            }
        });
    }

    fn start_listening_incoming_bidirectional_streams(
        transport: Rc<WebTransport>,
        streams: ReadableStream,
        callback: Callback<WebTransportBidirectionalStream>,
    ) {
        let read_result: ReadableStreamDefaultReader = streams.get_reader().unchecked_into();
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let read_result = JsFuture::from(read_result.read()).await;
                match read_result {
                    Err(e) => {
                        let mut reason = WebTransportCloseInfo::default();
                        reason.reason(
                            format!("Failed to read incoming unidirectional streams {e:?}")
                                .as_str(),
                        );
                        transport.close_with_close_info(&reason);
                        break;
                    }
                    Ok(result) => {
                        let done = Reflect::get(&result, &JsString::from("done"))
                            .unwrap()
                            .unchecked_into::<Boolean>();
                        if let Ok(value) = Reflect::get(&result, &JsString::from("value")) {
                            if value.is_undefined() {
                                break;
                            }
                            let value: WebTransportBidirectionalStream = value.unchecked_into();
                            callback.emit(value);
                        }
                        if done.is_truthy() {
                            break;
                        }
                    }
                }
            }
        });
    }

    fn connect_common(
        url: &str,
        notification: &Callback<WebTransportStatus>,
    ) -> Result<ConnectCommon, WebTransportError> {
        let transport = WebTransport::new(url);
        let transport = transport.map_err(|e| {
            WebTransportError::CreationError(format!("Failed to create WebTransport: {e:?}"))
        })?;

        let notify = notification.clone();

        let opened_closure = Closure::wrap(Box::new(move |_| {
            notify.emit(WebTransportStatus::Opened);
        }) as Box<dyn FnMut(JsValue)>);
        let notify = notification.clone();
        let closed_closure = Closure::wrap(Box::new(move |e: JsValue| {
            notify.emit(WebTransportStatus::Closed(e));
        }) as Box<dyn FnMut(JsValue)>);
        let ready = transport
            .ready()
            .then(&opened_closure)
            .catch(&closed_closure);
        opened_closure.forget();
        let closed = transport.closed().then(&closed_closure);
        closed_closure.forget();

        {
            let listeners = [ready, closed];
            Ok(ConnectCommon(transport, listeners))
        }
    }
}
struct ConnectCommon(WebTransport, [Promise; 2]);

pub fn process_binary(bytes: &Uint8Array, callback: &Callback<Vec<u8>>) {
    let data = bytes.to_vec();
    callback.emit(data);
}

impl WebTransportTask {
    /// Sends data to a WebTransport connection.
    pub fn send_datagram(transport: Rc<WebTransport>, data: Vec<u8>) {
        let transport = transport;
        wasm_bindgen_futures::spawn_local(async move {
            let transport = transport.clone();
            let result: Result<(), anyhow::Error> = async move {
                let stream = transport.datagrams();
                let stream: WritableStream = stream.writable();
                let writer = stream.get_writer().map_err(|e| anyhow!("{:?}", e))?;
                let data = Uint8Array::from(data.as_slice());
                let _stream = JsFuture::from(writer.write_with_chunk(&data))
                    .await
                    .map_err(|e| anyhow::anyhow!("{:?}", e))?;
                writer.release_lock();
                Ok(())
            }
            .await;
            if let Err(e) = result {
                let e = e.to_string();
                log!("error: {}", e);
            }
        });
    }

    pub fn send_unidirectional_stream(transport: Rc<WebTransport>, data: Vec<u8>) {
        let transport = transport;
        wasm_bindgen_futures::spawn_local(async move {
            let transport = transport.clone();
            let result: Result<(), anyhow::Error> = async move {
                let stream = JsFuture::from(transport.create_unidirectional_stream()).await;
                let stream: WritableStream =
                    stream.map_err(|e| anyhow!("{:?}", e))?.unchecked_into();
                let writer = stream.get_writer().map_err(|e| anyhow!("{:?}", e))?;
                let data = Uint8Array::from(data.as_slice());
                let _ = JsFuture::from(writer.write_with_chunk(&data))
                    .await
                    .map_err(|e| anyhow::anyhow!("{:?}", e))?;
                writer.release_lock();
                JsFuture::from(stream.close())
                    .await
                    .map_err(|e| anyhow::anyhow!("{:?}", e))?;
                Ok(())
            }
            .await;
            if let Err(e) = result {
                let e = e.to_string();
                log!("error: {}", e);
            }
        });
    }

    pub fn send_bidirectional_stream(
        transport: Rc<WebTransport>,
        data: Vec<u8>,
        callback: Callback<Vec<u8>>,
    ) {
        let transport = transport;
        wasm_bindgen_futures::spawn_local(async move {
            let transport = transport.clone();
            let result: Result<(), anyhow::Error> = async move {
                let stream = JsFuture::from(transport.create_bidirectional_stream()).await;
                let stream: WebTransportBidirectionalStream =
                    stream.map_err(|e| anyhow!("{:?}", e))?.unchecked_into();
                let readable: ReadableStreamDefaultReader =
                    stream.readable().get_reader().unchecked_into();
                let (sender, receiver) = channel();
                wasm_bindgen_futures::spawn_local(async move {
                    loop {
                        let read_result = JsFuture::from(readable.read()).await;
                        match read_result {
                            Err(e) => {
                                let mut reason = WebTransportCloseInfo::default();
                                reason.reason(
                                    format!("Failed to read incoming stream {e:?}").as_str(),
                                );
                                transport.close_with_close_info(&reason);
                                break;
                            }
                            Ok(result) => {
                                let done = Reflect::get(&result, &JsString::from("done"))
                                    .unwrap()
                                    .unchecked_into::<Boolean>();
                                if done.is_truthy() {
                                    break;
                                }
                                let value: Uint8Array =
                                    Reflect::get(&result, &JsString::from("value"))
                                        .unwrap()
                                        .unchecked_into();
                                process_binary(&value, &callback);
                            }
                        }
                    }
                    sender.send(true).unwrap();
                });
                let writer = stream
                    .writable()
                    .get_writer()
                    .map_err(|e| anyhow!("{:?}", e))?;

                let data = Uint8Array::from(data.as_slice());
                let _ = JsFuture::from(writer.write_with_chunk(&data))
                    .await
                    .map_err(|e| anyhow::anyhow!("{:?}", e))?;
                JsFuture::from(writer.close())
                    .await
                    .map_err(|e| anyhow::anyhow!("{:?}", e))?;

                let _ = receiver.await;

                Ok(())
            }
            .await;
            if let Err(e) = result {
                let e = e.to_string();
                log!("error: {}", e);
            }
        });
    }
}
