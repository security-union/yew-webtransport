use anyhow::Error;
use chrono::{DateTime, Local};
use gloo_console::log;
use serde_derive::{Deserialize, Serialize};
use web_sys::HtmlTextAreaElement;
use web_sys::KeyboardEvent;
use yew::prelude::*;
use yew::TargetCast;
use yew::{html, Component, Context, Html};
use yew_webtransport::macros::Json;
use yew_webtransport::webtransport::{WebTransportService, WebTransportStatus, WebTransportTask};

const DEFAULT_URL: &str = "https://echo.webtransport.day";

pub enum Format {
    Json,
    Toml,
}

pub enum WsAction {
    Connect,
    SendData(),
    SetText(String),
    Disconnect,
    Lost,
}

pub enum Msg {
    WsAction(WsAction),
    WsReady(Result<WsResponse, Error>),
}

impl From<WsAction> for Msg {
    fn from(action: WsAction) -> Self {
        Msg::WsAction(action)
    }
}

/// This type is used as a request which sent to webtransport connection.
#[derive(Serialize, Debug)]
struct WsRequest {
    value: Vec<u8>,
}

/// This type is an expected response from a webtransport connection.
#[derive(Deserialize, Debug)]
pub struct WsResponse {
    value: Vec<u8>,
}

pub struct Model {
    pub fetching: bool,
    pub transport: Option<WebTransportTask>,
    pub log: Vec<String>,
    pub endpoint: String,
    pub text: String,
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
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::WsAction(action) => match action {
                WsAction::Connect => {
                    let callback = ctx.link().callback(|Json(data)| Msg::WsReady(data));
                    let notification = ctx.link().batch_callback(|status| match status {
                        WebTransportStatus::Opened => None,
                        WebTransportStatus::Closed | WebTransportStatus::Error => {
                            Some(WsAction::Lost.into())
                        }
                    });
                    let endpoint = self.endpoint.clone();
                    let task = WebTransportService::connect(&endpoint, callback, notification);
                    self.transport = match task {
                        Ok(task) => Some(task),
                        Err(err) => {
                            log!("Failed to connect to WebTransport:");
                            None
                        }
                    };
                    true
                }
                WsAction::SendData() => {
                    let text = self.text.clone().into_bytes();
                    let request = WsRequest { value: text };
                    if let Some(transport) = self.transport.as_ref() {
                        WebTransportTask::send_binary(transport.transport.clone(), Json(&request));
                    };

                    false
                }
                WsAction::Disconnect => {
                    self.transport.take();
                    true
                }
                WsAction::SetText(text) => {
                    self.text = text;
                    true
                }
                WsAction::Lost => {
                    self.transport = None;
                    true
                }
            },
            Msg::WsReady(response) => {
                let data = response.map(|data| data.value).ok();
                let update = format!("{} - resp datagram: {:?}", get_time(), data);
                self.log.splice(0..0, vec![update]);
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div>
                <nav class="menu">
                </nav>
                <div id="tool">
                    <img class="banner" src="/assets/banner.jpeg"/>
                    <h1>{"Yew-WebTransport test client"}</h1>
                    <div>
                        <h2>{"Establish WebTransport connection"}</h2>
                        <div class="input-line">
                            <label for="url">{"URL:"}</label>
                            <input type="text" name="url" id="url" value={self.endpoint.clone()}/>
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
                            })} name="data" id="data"></textarea>
                            <div>
                                <input type="radio" name="sendtype" id="datagram" checked=false value="datagram"/>
                                <label for="datagram">{"Send a datagram"}</label>
                            </div>
                            <div>
                                <input type="radio" name="sendtype" id="unidi-stream" value="unidi"/>
                                <label for="unidi-stream">{"Open a unidirectional stream"}</label>
                            </div>
                            <div>
                                <input type="radio" name="sendtype" id="bidi-stream" value="bidi"/>
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

fn get_time() -> String {
    let now: DateTime<Local> = Local::now();
    now.format("%H:%M:%S").to_string()
}

fn main() {
    yew::Renderer::<Model>::new().render();
}
