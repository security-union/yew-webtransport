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
use anyhow::{Error, anyhow};
use wasm_bindgen_futures::JsFuture;
use std::fmt;
use thiserror::Error as ThisError;
use yew::callback::Callback;

use gloo::events::EventListener;
use js_sys::Uint8Array;
use wasm_bindgen::JsCast;
use web_sys::{BinaryType, Event, MessageEvent, WebTransport, WebTransportSendStream, WritableStream};

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
    ws: WebTransport,
    notification: Callback<WebTransportStatus>,
    #[allow(dead_code)]
    listeners: [EventListener; 4],
}

impl WebTransportTask {
    fn new(
        ws: WebTransport,
        notification: Callback<WebTransportStatus>,
        listener_0: EventListener,
        listeners: [EventListener; 3],
    ) -> WebTransportTask {
        let [listener_1, listener_2, listener_3] = listeners;
        WebTransportTask {
            ws,
            notification,
            listeners: [listener_0, listener_1, listener_2, listener_3],
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
        let ConnectCommon(ws, listeners) = Self::connect_common(url, &notification)?;
        let listener = EventListener::new(&ws, "message", move |event: &Event| {
            let event = event.dyn_ref::<MessageEvent>().unwrap();
            process_both(&event, &callback);
        });
        Ok(WebTransportTask::new(ws, notification, listener, listeners))
    }

    /// Connects to a server through a WebTransport connection, like connect,
    /// but only processes binary frames. Text frames are silently
    /// ignored. Needs two functions to generate data and notification
    /// messages.
    pub fn connect_binary<OUT: 'static>(
        url: &str,
        callback: Callback<OUT>,
        notification: Callback<WebTransportStatus>,
    ) -> Result<WebTransportTask, WebTransportError>
    where
        OUT: From<Binary>,
    {
        let ConnectCommon(ws, listeners) = Self::connect_common(url, &notification)?;
        let listener = EventListener::new(&ws, "message", move |event: &Event| {
            let event = event.dyn_ref::<MessageEvent>().unwrap();
            process_binary(&event, &callback);
        });
        Ok(WebTransportTask::new(ws, notification, listener, listeners))
    }

    /// Connects to a server through a WebTransport connection, like connect,
    /// but only processes text frames. Binary frames are silently
    /// ignored. Needs two functions to generate data and notification
    /// messages.
    pub fn connect_text<OUT: 'static>(
        url: &str,
        callback: Callback<OUT>,
        notification: Callback<WebTransportStatus>,
    ) -> Result<WebTransportTask, WebTransportError>
    where
        OUT: From<Text>,
    {
        let ConnectCommon(ws, listeners) = Self::connect_common(url, &notification)?;
        let listener = EventListener::new(&ws, "message", move |event: &Event| {
            let event = event.dyn_ref::<MessageEvent>().unwrap();
            process_text(&event, &callback);
        });
        Ok(WebTransportTask::new(ws, notification, listener, listeners))
    }

    fn connect_common(
        url: &str,
        notification: &Callback<WebTransportStatus>,
    ) -> Result<ConnectCommon, WebTransportError> {
        let ws = WebTransport::new(url);

        let ws = ws.map_err(|ws_error| {
            WebTransportError::CreationError(
                ws_error
                    .unchecked_into::<js_sys::Error>()
                    .to_string()
                    .as_string()
                    .unwrap(),
            )
        })?;

        ws.set_binary_type(BinaryType::Arraybuffer);
        let notify = notification.clone();
        let listener_open = move |_: &Event| {
            notify.emit(WebTransportStatus::Opened);
        };
        let notify = notification.clone();
        let listener_close = move |_: &Event| {
            notify.emit(WebTransportStatus::Closed);
        };
        let notify = notification.clone();
        let listener_error = move |_: &Event| {
            notify.emit(WebTransportStatus::Error);
        };
        {
            let listeners = [
                EventListener::new(&ws, "open", listener_open),
                EventListener::new(&ws, "close", listener_close),
                EventListener::new(&ws, "error", listener_error),
            ];
            Ok(ConnectCommon(ws, listeners))
        }
    }
}

struct ConnectCommon(WebTransport, [EventListener; 3]);

fn process_binary<OUT: 'static>(event: &MessageEvent, callback: &Callback<OUT>)
where
    OUT: From<Binary>,
{
    let bytes = if !event.data().is_string() {
        Some(event.data())
    } else {
        None
    };

    let data = if let Some(bytes) = bytes {
        let bytes: Vec<u8> = Uint8Array::new(&bytes).to_vec();
        Ok(bytes)
    } else {
        Err(FormatError::ReceivedTextForBinary.into())
    };

    let out = OUT::from(data);
    callback.emit(out);
}

fn process_text<OUT: 'static>(event: &MessageEvent, callback: &Callback<OUT>)
where
    OUT: From<Text>,
{
    let text = event.data().as_string();

    let data = if let Some(text) = text {
        Ok(text)
    } else {
        Err(FormatError::ReceivedBinaryForText.into())
    };

    let out = OUT::from(data);
    callback.emit(out);
}

fn process_both<OUT: 'static>(event: &MessageEvent, callback: &Callback<OUT>)
where
    OUT: From<Text> + From<Binary>,
{
    let is_text = event.data().is_string();
    if is_text {
        process_text(event, callback);
    } else {
        process_binary(event, callback);
    }
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
                    let stream = JsFuture::from(self.ws.create_unidirectional_stream()).await.map_err(|e| anyhow::anyhow!("e.as_str(sdf)"))?;
                    let stream: WritableStream = stream.unchecked_into();
                    let stream = stream.get_writer().map_err(|e| anyhow!("error"))?;
                    let stream = JsFuture::from(stream.write_with_chunk(&body.into())).await.map_err(|e| anyhow::anyhow!("e.as_str(sdf)"))?;
                    Ok(())
                }.await;
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
            let result = self.ws.send_with_u8_array(&body);

            if result.is_err() {
                self.notification.emit(WebTransportStatus::Error);
            }
        }
    }
}

impl WebTransportTask {
    fn is_active(&self) -> bool {
        matches!(
            self.ws.ready_state(),
            WebTransport::CONNECTING | WebTransport::OPEN
        )
    }
}

impl Drop for WebTransportTask {
    fn drop(&mut self) {
        if self.is_active() {
            self.ws.close().ok();
        }
    }
}
