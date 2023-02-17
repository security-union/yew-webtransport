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
use std::fmt;
use thiserror::Error as ThisError;
use wasm_bindgen_futures::JsFuture;
use yew::callback::Callback;

use gloo::events::EventListener;
use js_sys::{Boolean, JsString, Promise, Reflect, Uint8Array};
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::{
    console::log, BinaryType, Event, MessageEvent, MouseEvent, ReadableStreamDefaultReader,
    ReadableStreamReadResult, WebTransport, WebTransportSendStream, WritableStream,
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
    transport: WebTransport,
    notification: Callback<WebTransportStatus>,
    #[allow(dead_code)]
    listeners: [Promise; 2],
}

impl WebTransportTask {
    fn new(
        transport: WebTransport,
        notification: Callback<WebTransportStatus>,
        listeners: [Promise; 2],
    ) -> WebTransportTask {
        WebTransportTask {
            transport,
            notification,
            listeners
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
    pub fn connect<OUT: 'static>(
        url: &str,
        callback: Callback<OUT>,
        notification: Callback<WebTransportStatus>,
    ) -> Result<WebTransportTask, WebTransportError>
    where
        OUT: From<Text> + From<Binary>,
    {
        let ConnectCommon(transport, listeners) = Self::connect_common(url, &notification)?;
        let incoming_uni = transport.incoming_unidirectional_streams();
        let incoming_uni: ReadableStreamDefaultReader = incoming_uni.get_reader().unchecked_into();
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let read_result = JsFuture::from(incoming_uni.read()).await;
                match read_result {
                    Err(e) => {
                        // log!("Error reading from incoming_uni: {:?}", e);
                        break;
                    }
                    Ok(result) => {
                        let done = Reflect::get(&result, &JsString::from("done"))
                            .unwrap()
                            .unchecked_into::<Boolean>();
                        if done.is_truthy() {
                            // log::info!("Done reading from incoming_uni");
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
            transport,
            notification,
            listeners,
        ))
    }

    fn connect_common(
        url: &str,
        notification: &Callback<WebTransportStatus>,
    ) -> Result<ConnectCommon, WebTransportError> {
        let transport = WebTransport::new(url);

        let transport = transport.map_err(|ws_error| {
            WebTransportError::CreationError(
                ws_error
                    .unchecked_into::<js_sys::Error>()
                    .to_string()
                    .as_string()
                    .unwrap(),
            )
        })?;

        let notify = notification.clone();
        let ready = transport
            .ready()
            .then(&Closure::wrap(Box::new(move |result| {
                notify.emit(WebTransportStatus::Opened);
            }) as Box<dyn FnMut(JsValue)>));

        let notify = notification.clone();
        let closed = transport
            .closed()
            .then(&Closure::wrap(Box::new(move |result| {
                notify.emit(WebTransportStatus::Closed);
            }) as Box<dyn FnMut(JsValue)>));

        {
            let listeners = [ready, closed];
            Ok(ConnectCommon(transport, listeners))
        }
    }
}

struct ConnectCommon(WebTransport, [Promise; 2]);

fn process_binary<OUT: 'static>(bytes: &Uint8Array, callback: &Callback<OUT>)
where
    OUT: From<Binary>,
{
    let data = Ok(bytes.to_vec());
    let out = OUT::from(data);
    callback.emit(out);
}

impl WebTransportTask {
    /// Sends data to a WebTransport connection.
    pub fn send<IN>(&mut self, data: IN)
    where
        IN: Into<Text>,
    {
        if let Ok(body) = data.into() {
            wasm_bindgen_futures::spawn_local(async move {
                let result: Result<(), anyhow::Error> = async move {
                    let stream = JsFuture::from(self.transport.create_unidirectional_stream())
                        .await
                        .map_err(|e| anyhow::anyhow!("e.as_str(sdf)"))?;
                    let stream: WritableStream = stream.unchecked_into();
                    let stream = stream.get_writer().map_err(|e| anyhow!("error"))?;
                    let stream = JsFuture::from(stream.write_with_chunk(&body.into()))
                        .await
                        .map_err(|e| anyhow::anyhow!("e.as_str(sdf)"))?;
                    Ok(())
                }
                .await;
                if result.is_err() {
                    self.notification.emit(WebTransportStatus::Error);
                }
            });
        }
    }

    /// Sends binary data to a WebTransport connection.
    pub fn send_binary<IN>(&mut self, data: IN)
    where
        IN: Into<Binary>,
    {
        if let Ok(body) = data.into() {
            let result = self.transport.send_with_u8_array(&body);

            if result.is_err() {
                self.notification.emit(WebTransportStatus::Error);
            }
        }
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
