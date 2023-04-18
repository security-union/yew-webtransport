use chrono::Local;
use web_sys::HtmlInputElement;
use web_sys::HtmlTextAreaElement;
use web_sys::KeyboardEvent;
use yew::prelude::*;
use yew::TargetCast;
use yew::{html, Component, Context, Html};
use yew_webtransport::webtransport::{WebTransportService, WebTransportStatus, WebTransportTask};

const DEFAULT_URL: &str = std::env!("WS_URL");

pub enum WsAction {
    Connect,
    SendData(),
    SetText(String),
    SetMessageType(WebTransportMessageType),
    SetUrl(String),
    Log(String),
    Disconnect,
    Lost,
}

pub enum Msg {
    WsAction(WsAction),
    WsReady(Vec<u8>, WebTransportMessageType),
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
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::WsAction(action) => match action {
                WsAction::Connect => {
                    let on_datagram = ctx
                        .link()
                        .callback(|d| Msg::WsReady(d, WebTransportMessageType::Datagram));
                    let on_unidirectional_stream = ctx.link().callback(|d| {
                        Msg::WsReady(d, WebTransportMessageType::UnidirectionalStream)
                    });
                    let notification = ctx.link().batch_callback(|status| match status {
                        WebTransportStatus::Opened => {
                            Some(WsAction::Log(String::from("Connected")).into())
                        }
                        WebTransportStatus::Closed | WebTransportStatus::Error => {
                            Some(WsAction::Lost.into())
                        }
                    });
                    let endpoint = self.endpoint.clone();
                    let task = WebTransportService::connect(
                        &endpoint,
                        on_datagram,
                        on_unidirectional_stream,
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
                                    Msg::WsReady(d, WebTransportMessageType::BidirectionalStream)
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
                WsAction::Lost => {
                    self.transport = None;
                    ctx.link()
                        .send_message(WsAction::Log(String::from("Connection lost")));
                    true
                }
            },
            Msg::WsReady(response, message_type) => {
                let data = String::from_utf8(response).unwrap();
                ctx.link().send_message(WsAction::Log(format!(
                    "We received {data:?} through {message_type:?}"
                )));
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
