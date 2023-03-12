use anyhow::Error;
use serde_derive::{Deserialize, Serialize};
use yew_webtransport::macros::Json;
use chrono::{DateTime, Local};


use gloo_console::log;
use yew::{html, Component, Context, Html};
use yew_webtransport::webtransport::{WebTransportService, WebTransportStatus, WebTransportTask};

const DEFAULT_URL: &str = "https://echo.webtransport.day";

type AsBinary = bool;

pub enum Format {
    Json,
    Toml,
}

pub enum WsAction {
    Connect,
    SendData(AsBinary),
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
    value: u32,
}

/// This type is an expected response from a webtransport connection.
#[derive(Deserialize, Debug)]
pub struct WsResponse {
    value: u32,
}

pub struct Model {
    pub fetching: bool,
    pub transport: Option<WebTransportTask>,
    pub log: Vec<String>,
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            fetching: false,
            transport: None,
            log: vec![],
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::WsAction(action) => match action {
                WsAction::Connect => {
                    let callback = ctx.link().callback(|Json(data)| Msg::WsReady(data));
                    let notification = ctx.link().batch_callback(|status| match status {
                        WebTransportStatus::Opened => {
                            None
                        },
                        WebTransportStatus::Closed | WebTransportStatus::Error => {
                            Some(WsAction::Lost.into())
                        }
                    });
                    let task = WebTransportService::connect(
                        DEFAULT_URL,
                        callback,
                        notification,
                    );
                    self.transport = match task {
                        Ok(task) => Some(task),
                        Err(err) => {
                            log!("Failed to connect to WebTransport:");
                            None
                        }
                    };
                    true
                }
                WsAction::SendData(binary) => {
                    let request = WsRequest { value: 321 };
                    if let Some(transport) = self.transport.as_ref() {
                        WebTransportTask::send_binary(transport.transport.clone(), Json(&request));
                    };

                    false
                }
                WsAction::Disconnect => {
                    self.transport.take();
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
                    <button disabled={self.transport.is_none()}
                            onclick={ctx.link().callback(|_| WsAction::SendData(false))}>
                        { "Send To WebTransport" }
                    </button>
                    <button disabled={self.transport.is_none()}
                            onclick={ctx.link().callback(|_| WsAction::SendData(true))}>
                        { "Send To WebTransport [binary]" }
                    </button>
                </nav>
                <div id="tool">
                    <img class="banner" src="/assets/banner.jpeg"/>
                    <h1>{"WebTransport over HTTP/3 client"}</h1>
                    <div>
                        <h2>{"Establish WebTransport connection"}</h2>
                        <div class="input-line">
                            <label for="url">{"URL:"}</label>
                            <input type="text" name="url" id="url" value={DEFAULT_URL.to_string()}/>
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
                            <textarea name="data" id="data"></textarea>
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
                            <input type="button" id="send" name="send" disabled=false value="Send data"/>
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
