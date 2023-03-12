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

use gloo_console::log;
use js_sys::{Boolean, JsString, Promise, Reflect, Uint8Array};
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::{ReadableStreamDefaultReader, WebTransport, WritableStream};

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
    Closed,
    /// Fired when a WebTransport connection has failed.
    Error,
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
        callback: Callback<Vec<u8>>,
        notification: Callback<WebTransportStatus>,
    ) -> Result<WebTransportTask, WebTransportError> {
        let ConnectCommon(transport, listeners) = Self::connect_common(url, &notification)?;
        let datagrams = transport.datagrams();
        let incoming_datagrams: ReadableStreamDefaultReader =
            datagrams.readable().get_reader().unchecked_into();
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let read_result = JsFuture::from(incoming_datagrams.read()).await;
                match read_result {
                    Err(e) => {
                        log!("Error reading from stream: {:?}", e);
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

        Ok(WebTransportTask::new(
            transport.into(),
            notification,
            listeners,
        ))
    }

    fn connect_common(
        url: &str,
        notification: &Callback<WebTransportStatus>,
    ) -> Result<ConnectCommon, WebTransportError> {
        let transport = WebTransport::new(url);
        let transport = transport.unwrap();

        let notify = notification.clone();

        let closure = Closure::wrap(Box::new(move |e| {
            notify.emit(WebTransportStatus::Opened);
        }) as Box<dyn FnMut(JsValue)>);
        let ready = transport.ready().then(&closure);
        closure.forget();

        let notify = notification.clone();
        let closed_closure = Closure::wrap(Box::new(move |e| {
            notify.emit(WebTransportStatus::Closed);
        }) as Box<dyn FnMut(JsValue)>);
        let closed = transport.closed().then(&closed_closure);
        closed_closure.forget();

        {
            let listeners = [ready, closed];
            Ok(ConnectCommon(transport, listeners))
        }
    }
}
struct ConnectCommon(WebTransport, [Promise; 2]);

fn process_binary(bytes: &Uint8Array, callback: &Callback<Vec<u8>>) {
    let data = bytes.to_vec();
    callback.emit(data);
}

impl WebTransportTask {
    /// Sends data to a WebTransport connection.
    pub fn send_binary(transport: Rc<WebTransport>, data: Vec<u8>) {
        let transport = transport.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let transport = transport.clone();
            let result: Result<(), anyhow::Error> = async move {
                let stream = transport.datagrams();
                let stream: WritableStream = stream.writable();
                let writer = stream.get_writer().map_err(|e| anyhow!("{:?}", e))?;
                let data = Uint8Array::from(data.as_slice());
                let stream = JsFuture::from(writer.write_with_chunk(&data))
                    .await
                    .map_err(|e| anyhow::anyhow!("{:?}", e))?;
                writer.release_lock();
                Ok(())
            }
            .await;
            if let Err(e) = result {
                let e = e.to_string();
                log!("error: {}", e);
                // self.notification.emit(WebTransportStatus::Error);
            }
        });
    }
}

impl WebTransportTask {
    fn is_active(&self) -> bool {
        false
    }
}

impl Drop for WebTransportTask {
    fn drop(&mut self) {
        if self.is_active() {
            self.transport.close();
        }
    }
}
