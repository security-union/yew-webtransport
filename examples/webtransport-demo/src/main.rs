use chrono::Local;
use gloo_console::log;
use js_sys::{Boolean, JsString, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlInputElement;
use web_sys::HtmlTextAreaElement;
use web_sys::KeyboardEvent;
use web_sys::ReadableStreamDefaultReader;
use web_sys::WebTransportBidirectionalStream;
use web_sys::WebTransportCloseInfo;
use web_sys::WebTransportReceiveStream;

use yew::prelude::*;
use yew::TargetCast;
use yew::{html, Component, Context, Html};
use yew_webtransport::webtransport::process_binary;
use yew_webtransport::webtransport::{WebTransportService, WebTransportStatus, WebTransportTask};

const DEFAULT_URL: &str = std::env!("WS_URL");

pub enum WsAction {
    Connect,
    SendData(),
    SetText(String),
    SetUrl(String),
    SetMessageType(WebTransportMessageType),
    Log(String),
    Disconnect,
    Lost(String),
    Connected,
}

pub enum Msg {
    WsAction(WsAction),
    OnDatagram(Vec<u8>),
    OnUniStream(WebTransportReceiveStream),
    OnBidiStream(WebTransportBidirectionalStream),
    OnMessage(Vec<u8>, WebTransportMessageType),
}

impl From<WsAction> for Msg {
    fn from(action: WsAction) -> Self {
        Msg::WsAction(action)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebTransportMessageType {
    Datagram,
    UnidirectionalStream,
    BidirectionalStream,
    Unknown,
}

pub struct Model {
    pub fetching: bool,
    pub transport: Option<WebTransportTask>,
    pub connected: bool,
    pub log: Vec<String>,
    pub endpoint: String,
    pub text: String,
    pub message_type: WebTransportMessageType,
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            fetching: false,
            transport: None,
            log: vec![],
            endpoint: DEFAULT_URL.to_string(),
            text: "".to_string(),
            message_type: WebTransportMessageType::Datagram,
            connected: false,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::WsAction(action) => match action {
                WsAction::Connect => {
                    let on_datagram = ctx.link().callback(Msg::OnDatagram);
                    let on_unidirectional_stream = ctx.link().callback(Msg::OnUniStream);
                    let on_bidirectional_stream = ctx.link().callback(Msg::OnBidiStream);
                    let notification = ctx.link().batch_callback(|status| match status {
                        WebTransportStatus::Opened => Some(WsAction::Connected.into()),
                        WebTransportStatus::Closed(reason) | WebTransportStatus::Error(reason) => {
                            let formatted_reason = format!("{:?}", reason);
                            Some(WsAction::Lost(formatted_reason).into())
                        }
                    });
                    let endpoint = self.endpoint.clone();
                    let task = WebTransportService::connect(
                        &endpoint,
                        on_datagram,
                        on_unidirectional_stream,
                        on_bidirectional_stream,
                        notification,
                    );
                    self.transport = match task {
                        Ok(task) => Some(task),
                        Err(err) => {
                            ctx.link().send_message(WsAction::Log(err.to_string()));
                            None
                        }
                    };
                    true
                }
                WsAction::SendData() => {
                    let text = self.text.clone();
                    let message_type = self.message_type.clone();
                    if let Some(transport) = self.transport.as_ref() {
                        ctx.link().send_message(WsAction::Log(format!(
                            "Sending: {:?} using {:?}",
                            &text, message_type
                        )));
                        let text = text.into_bytes();
                        match message_type {
                            WebTransportMessageType::Datagram => {
                                WebTransportTask::send_datagram(transport.transport.clone(), text);
                            }
                            WebTransportMessageType::UnidirectionalStream => {
                                WebTransportTask::send_unidirectional_stream(
                                    transport.transport.clone(),
                                    text,
                                );
                            }
                            WebTransportMessageType::BidirectionalStream => {
                                let on_bidirectional_stream = ctx.link().callback(|d| {
                                    Msg::OnMessage(d, WebTransportMessageType::BidirectionalStream)
                                });
                                WebTransportTask::send_bidirectional_stream(
                                    transport.transport.clone(),
                                    text,
                                    on_bidirectional_stream,
                                );
                            }
                            WebTransportMessageType::Unknown => {}
                        }
                    };

                    false
                }
                WsAction::Disconnect => {
                    let connection = self.transport.take();
                    if let Some(connection) = connection {
                        connection.transport.close()
                    }
                    true
                }
                WsAction::SetText(text) => {
                    self.text = text;
                    true
                }
                WsAction::SetUrl(url) => {
                    self.endpoint = url;
                    true
                }
                WsAction::SetMessageType(message_type) => {
                    self.message_type = message_type;
                    true
                }
                WsAction::Log(text) => {
                    let text = format!("{}: {}", Local::now().format("%H:%M:%S%.3f"), text);
                    self.log.splice(0..0, vec![text]);
                    true
                }
                WsAction::Connected => {
                    self.connected = true;
                    ctx.link()
                        .send_message(WsAction::Log("Connected".to_string()));
                    true
                }
                WsAction::Lost(reason) => {
                    self.connected = false;
                    self.transport = None;
                    ctx.link()
                        .send_message(WsAction::Log(format!("Connection lost ({})", reason)));
                    true
                }
            },
            Msg::OnMessage(response, message_type) => {
                let data = String::from_utf8(response).unwrap();
                ctx.link().send_message(WsAction::Log(format!(
                    "We received {data:?} through {message_type:?}"
                )));
                true
            }
            Msg::OnDatagram(datagram) => {
                // With datagrams there's no need to read from a stream, so we just rebroadcast another message.
                ctx.link()
                    .send_message(Msg::OnMessage(datagram, WebTransportMessageType::Datagram));
                false
            }
            Msg::OnBidiStream(stream) => {
                // TODO: Read from the stream and do something useful with the data.
                log!("OnBidiStream: ", &stream);
                let callback = ctx
                    .link()
                    .callback(|d| Msg::OnMessage(d, WebTransportMessageType::BidirectionalStream));
                let readable: ReadableStreamDefaultReader =
                    stream.readable().get_reader().unchecked_into();
                wasm_bindgen_futures::spawn_local(async move {
                    loop {
                        let read_result = JsFuture::from(readable.read()).await;
                        match read_result {
                            Err(e) => {
                                let mut reason = WebTransportCloseInfo::default();
                                reason.reason(
                                    format!("Failed to read incoming datagrams {e:?}").as_str(),
                                );
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
                });
                true
            }
            Msg::OnUniStream(stream) => {
                // TODO: Read from the stream and do something useful with the data.
                log!("OnUniStream: ", &stream);
                let incoming_datagrams: ReadableStreamDefaultReader =
                    stream.get_reader().unchecked_into();
                let callback = ctx
                    .link()
                    .callback(|d| Msg::OnMessage(d, WebTransportMessageType::UnidirectionalStream));
                wasm_bindgen_futures::spawn_local(async move {
                    loop {
                        let read_result = JsFuture::from(incoming_datagrams.read()).await;
                        match read_result {
                            Err(e) => {
                                let mut reason = WebTransportCloseInfo::default();
                                reason.reason(
                                    format!("Failed to read incoming datagrams {e:?}").as_str(),
                                );
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
                });
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let message_type = self.message_type.clone();
        html! {
            <div>
                <nav class="menu">
                </nav>
                <div id="tool">
                    <img class="banner" src="./assets/banner.jpeg"/>
                    <h1>{"Yew-WebTransport test client"}</h1>
                    <div>
                        <h2>{"Establish WebTransport connection"}</h2>
                        <div class="input-line">
                            <label for="url">{"URL:"}</label>
                            <input type="text"
                                name="url"
                                id="url"
                                value={self.endpoint.clone()}
                                disabled={self.transport.is_some()}
                                onkeyup={ctx.link().callback(|e: KeyboardEvent| {
                                    let input = e.target_dyn_into::<HtmlInputElement>().unwrap();
                                    let text = input.value();
                                    WsAction::SetUrl(text)
                                })}/>
                            <input type="button"
                                id="connect"
                                disabled={self.transport.is_some()}
                                value="Connect"
                                onclick={ctx.link().callback(|_| WsAction::Connect)}/>
                            <input type="button"
                                id="connect"
                                disabled={self.transport.is_none()}
                                value="Disconnect"
                                onclick={ctx.link().callback(|_| WsAction::Disconnect)}/>
                        </div>
                    </div>
                    <div>
                        <h2>{"Send data over WebTransport"}</h2>
                        <form name="sending">
                            <textarea onkeyup={ctx.link().callback(|e: KeyboardEvent| {
                                let input = e.target_dyn_into::<HtmlTextAreaElement>().unwrap();
                                let text = input.value();
                                WsAction::SetText(text)
                            })} name="data" id="data" disabled={self.transport.is_none()}></textarea>
                            <div>
                                <input type="radio" name="sendtype" id="datagram" onchange={ctx.link().callback(|e: Event|{
                                    let input = e.target_dyn_into::<HtmlInputElement>().unwrap();
                                    if input.checked() {
                                        WsAction::SetMessageType(WebTransportMessageType::Datagram)
                                    } else {
                                        WsAction::SetMessageType(WebTransportMessageType::Unknown)
                                    }
                                })} checked={message_type==WebTransportMessageType::Datagram} value="datagram"/>
                                <label for="datagram">{"Send a datagram"}</label>
                            </div>
                            <div>
                                <input type="radio" name="sendtype" id="unidi-stream" value="unidi" onchange={ctx.link().callback(|e: Event|{
                                    let input = e.target_dyn_into::<HtmlInputElement>().unwrap();
                                    if input.checked() {
                                        WsAction::SetMessageType(WebTransportMessageType::UnidirectionalStream)
                                    } else {
                                        WsAction::SetMessageType(WebTransportMessageType::Unknown)
                                    }
                                })} checked={message_type==WebTransportMessageType::UnidirectionalStream}/>
                                <label for="unidi-stream">{"Open a unidirectional stream (** test server does not echo)"}</label>
                            </div>
                            <div>
                                <input type="radio" name="sendtype" id="bidi-stream" value="bidi" onchange={ctx.link().callback(|e: Event|{
                                    let input = e.target_dyn_into::<HtmlInputElement>().unwrap();
                                    if input.checked() {
                                        WsAction::SetMessageType(WebTransportMessageType::BidirectionalStream)
                                    } else {
                                        WsAction::SetMessageType(WebTransportMessageType::Unknown)
                                    }
                                })} checked={message_type==WebTransportMessageType::BidirectionalStream}/>
                                <label for="bidi-stream">{"Open a bidirectional stream"}</label>
                            </div>
                            <input type="button"
                                id="send"
                                name="send"
                                disabled={self.transport.is_none()}
                                value="Send data"
                                onclick={ctx.link().callback(|_| WsAction::SendData())}/>
                        </form>
                    </div>
                    <div>
                        <h2>{"Event log"}</h2>
                        <ul id="event-log">
                        { for self.log.iter().map(|log| html! { <li>{ log }</li> }) }
                        </ul>
                    </div>
                </div>
            </div>
        }
    }
}

fn main() {
    yew::Renderer::<Model>::new().render();
}
