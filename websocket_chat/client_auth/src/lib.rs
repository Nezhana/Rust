use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{WebSocket, MessageEvent, ErrorEvent, HtmlInputElement};
use yew::prelude::*;
use gloo_net::http::Request;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ChatMessage {
    username: String,
    content: String,
    timestamp: String,
}

#[derive(Clone)]
enum Msg {
    Connect,
    Disconnect,
    SendMessage,
    UpdateUsername(String),
    UpdateMessageInput(String),
    UpdatePassword(String),
    ReceiveMessage(ChatMessage),
    ConnectionError(String),
    ConnectionClosed,
    Login,
    Register,
    UpdateToken(String),
}

#[derive(Default)]
struct ChatApp {
    websocket: Option<WebSocket>,
    messages: Vec<ChatMessage>,
    username: String,
    password: String,
    message_input: String,
    is_connected: bool,
    token: Option<String>,
    auth_error: Option<String>,
}

impl Component for ChatApp {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Self::default()
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Connect => {
                if let Some(token) = &self.token {
                    match WebSocket::new("ws://localhost:8080") {
                        Ok(ws) => {
                            let link = ctx.link().clone();

                            let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
                                if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                                    if let Ok(message) = serde_json::from_str::<ChatMessage>(&txt.as_string().unwrap_or_default()) {
                                        link.send_message(Msg::ReceiveMessage(message));
                                    }
                                }
                            }) as Box<dyn Fn(MessageEvent)>);

                            let onerror_callback = {
                                let link = ctx.link().clone();
                                Closure::wrap(Box::new(move |e: ErrorEvent| {
                                    link.send_message(Msg::ConnectionError(e.message()));
                                }) as Box<dyn Fn(ErrorEvent)>)
                            };

                            let onclose_callback = {
                                let link = ctx.link().clone();
                                Closure::wrap(Box::new(move |_| {
                                    link.send_message(Msg::ConnectionClosed);
                                }) as Box<dyn Fn(JsValue)>)
                            };

                            ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
                            ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
                            ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));

                            onmessage_callback.forget();
                            onerror_callback.forget();
                            onclose_callback.forget();

                            // Send authentication token
                            let _ = ws.send_with_str(&format!("Bearer {}", token));

                            self.websocket = Some(ws);
                            self.is_connected = true;
                            true
                        }
                        Err(_) => false,
                    }
                } else {
                    self.auth_error = Some("Please login first.".to_string());
                    true
                }
            }
            Msg::Disconnect => {
                if let Some(ws) = &self.websocket {
                    let _ = ws.close();
                }
                self.websocket = None;
                self.is_connected = false;
                true
            }
            Msg::SendMessage => {
                if !self.message_input.is_empty() {
                    if let Some(ws) = &self.websocket {
                        let message = ChatMessage {
                            username: self.username.clone(),
                            content: self.message_input.clone(),
                            timestamp: js_sys::Date::new_0().to_iso_string().as_string().unwrap(),
                        };

                        if let Ok(json_message) = serde_json::to_string(&message) {
                            let _ = ws.send_with_str(&json_message);
                        }
                    }
                    self.message_input.clear();
                }
                true
            }
            Msg::UpdateUsername(username) => {
                self.username = username;
                true
            }
            Msg::UpdateMessageInput(input) => {
                self.message_input = input;
                true
            }
            Msg::UpdatePassword(password) => {
                self.password = password;
                true
            }
            Msg::UpdateToken(token) => {
                self.token = Some(token);
                self.auth_error = None;
                true
            }
            Msg::ReceiveMessage(message) => {
                self.messages.push(message);
                true
            }
            Msg::ConnectionError(_) | Msg::ConnectionClosed => {
                self.is_connected = false;
                self.websocket = None;
                true
            }
            Msg::Login => {
                let username = self.username.clone();
                let password = self.password.clone();
                let link = ctx.link().clone();

                wasm_bindgen_futures::spawn_local(async move {
                    let response = Request::post("http://localhost:3000/login")
                        .json(&serde_json::json!({ "username": username, "password": password }))
                        .unwrap()
                        .send()
                        .await;

                    match response {
                        Ok(res) => {
                            if let Ok(data) = res.json::<serde_json::Value>().await {
                                if let Some(token) = data.get("token").and_then(|t| t.as_str()) {
                                    link.send_message(Msg::UpdateToken(token.to_string()));

                                    // Fetch messages
                                    let fetch_response = Request::post("http://localhost:3000/messages")
                                        .header("Authorization", &format!("Bearer {}", token))
                                        .json(&serde_json::json!({ "username": username }))
                                        .unwrap()
                                        .send()
                                        .await;

                                    if let Ok(fetch_res) = fetch_response {
                                        if let Ok(messages) = fetch_res.json::<Vec<ChatMessage>>().await {
                                            for message in messages {
                                                link.send_message(Msg::ReceiveMessage(message));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => link.send_message(Msg::ConnectionError("Login failed".to_string())),
                    }
                });

                true
            }
            Msg::Register => {
                let username = self.username.clone();
                let password = self.password.clone();
                let link = ctx.link().clone();

                wasm_bindgen_futures::spawn_local(async move {
                    let response = Request::post("http://localhost:3000/register")
                        .json(&serde_json::json!({ "username": username, "password": password }))
                        .unwrap()
                        .send()
                        .await;

                    match response {
                        Ok(_) => link.send_message(Msg::Login),
                        Err(_) => link.send_message(Msg::ConnectionError("Registration failed".to_string())),
                    }
                });

                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="chat-app">
                <div class="auth-container">
                    <input
                        type="text"
                        placeholder="Username"
                        value={self.username.clone()}
                        oninput={ctx.link().callback(|e: InputEvent| {
                            let input: HtmlInputElement = e.target_unchecked_into();
                            Msg::UpdateUsername(input.value())
                        })}
                    />
                    <input
                        type="password"
                        placeholder="Password"
                        value={self.password.clone()}
                        oninput={ctx.link().callback(|e: InputEvent| {
                            let input: HtmlInputElement = e.target_unchecked_into();
                            Msg::UpdatePassword(input.value())
                        })}
                    />
                    <button class="auth-button" onclick={ctx.link().callback(|_| Msg::Login)}>{ "Login" }</button>
                    <button class="auth-button" onclick={ctx.link().callback(|_| Msg::Register)}>{ "Register" }</button>
                </div>
                if let Some(error) = &self.auth_error {
                    <div class="error-message">{ error }</div>
                }
                <div class="chat-container">
                    <h1>{ "Chat Room" }</h1>
                    <button class="connect-button"
                        onclick={ctx.link().callback(|_| Msg::Connect)}
                        disabled={self.is_connected}
                    >
                        { "Connect" }
                    </button>
                    <button class="disconnect-button"
                        onclick={ctx.link().callback(|_| Msg::Disconnect)}
                        disabled={!self.is_connected}
                    >
                        { "Disconnect" }
                    </button>
                    <div class="messages">
                        { for self.messages.iter().map(|msg| html! {
                            <div>
                                <strong>{ &msg.username }</strong>
                                <span>{ format!(": {}", &msg.content) }</span>
                                <span class="timestamp">{ format!("\t\t- {}", &msg.timestamp) }</span>
                            </div>
                        }) }
                    </div>
                    <div class="message-input">
                        <input
                            type="text"
                            placeholder="Message"
                            value={self.message_input.clone()}
                            oninput={ctx.link().callback(|e: InputEvent| {
                                let input: HtmlInputElement = e.target_unchecked_into();
                                Msg::UpdateMessageInput(input.value())
                            })}
                            disabled={!self.is_connected}
                        />
                        <button
                            onclick={ctx.link().callback(|_| Msg::SendMessage)}
                            disabled={!self.is_connected || self.message_input.is_empty()}
                        >
                            { "Send" }
                        </button>
                    </div>
                </div>
            </div>
        }
    }
}

#[wasm_bindgen]
pub fn run_app() -> Result<(), JsValue> {
    wasm_logger::init(wasm_logger::Config::default());
    yew::Renderer::<ChatApp>::new().render();
    Ok(())
}
