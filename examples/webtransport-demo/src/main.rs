use anyhow::Error;
use serde_derive::{Deserialize, Serialize};
use yew_webtransport::macros::Json;

use yew::{html, Component, Context, Html};
use yew_webtransport::webtransport::{WebTransportService, WebTransportStatus, WebTransportTask};

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
    pub data: Option<u32>,
    pub transport: Option<WebTransportTask>,
}

impl Model {
    fn view_data(&self) -> Html {
        if let Some(value) = self.data {
            html! {
                <p>{ value }</p>
            }
        } else {
            html! {
                <p>{ "Data hasn't fetched yet." }</p>
            }
        }
    }
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            fetching: false,
            data: None,
            transport: None,
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
                    let task = WebTransportService::connect(
                        "wss://echo.webtransport.events/",
                        callback,
                        notification,
                    )
                    .unwrap();
                    self.transport = Some(task);
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
                self.data = response.map(|data| data.value).ok();
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div>
                <nav class="menu">
                    { self.view_data() }
                    <button disabled={self.transport.is_some()}
                            onclick={ctx.link().callback(|_| WsAction::Connect)}>
                        { "Connect To WebTransport" }
                    </button>
                    <button disabled={self.transport.is_none()}
                            onclick={ctx.link().callback(|_| WsAction::SendData(false))}>
                        { "Send To WebTransport" }
                    </button>
                    <button disabled={self.transport.is_none()}
                            onclick={ctx.link().callback(|_| WsAction::SendData(true))}>
                        { "Send To WebTransport [binary]" }
                    </button>
                    <button disabled={self.transport.is_none()}
                            onclick={ctx.link().callback(|_| WsAction::Disconnect)}>
                        { "Close WebTransport connection" }
                    </button>
                </nav>
            </div>
        }
    }
}

fn main() {
    yew::Renderer::<Model>::new().render();
}
