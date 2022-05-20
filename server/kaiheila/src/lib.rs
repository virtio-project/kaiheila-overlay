pub mod message;

#[macro_use]
extern crate log;

use crate::message::{AccessTokenResponse, Cmd, Event, Message};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

const CLIENT_ID: &str = "15943749139034";
const DEFAULT_ENDPOINT: &str = "ws://127.0.0.1:5988/?url=";
const TOKEN_URL: &str = "https://www.kaiheila.cn/api/oauth2/token";

type WsMessage = tokio_tungstenite::tungstenite::Message;
type MessageHandler = oneshot::Sender<Result<Message, ClientError>>;
type EventHandlers = Vec<mpsc::UnboundedSender<Message>>;

/// Customize a [`Client`]
#[derive(Debug)]
pub struct ClientBuilder {
    client_id: String,
    endpoint: String,
    token_url: String,
    guild_id: Option<String>,
    channel_id: Option<String>,
}

/// Errors occurs during a [`Client`] building phase
#[derive(Debug, thiserror::Error)]
pub enum ClientBuildError {
    #[error("incomplete client builder")]
    MissingField,
    #[error("ws client error: {0}")]
    WsClientError(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("message error during initialization: {0}")]
    InitializationError(#[from] message::MessageError),
    #[error("error during request access_token: {0}")]
    GetTokenError(#[from] reqwest::Error),
}

/// Errors occurs from a [`Client`] lifetime
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("message could not be sent")]
    SenderError(Message),
    #[error("message could not be retrieved")]
    RecvError(#[from] tokio::sync::oneshot::error::RecvError),
    #[error("ws client error: {0}")]
    WsClientError(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("deserialization error: {0}")]
    DeserializationError(#[from] serde_json::Error),
}

/// A wrapped kaiheila client
pub struct Client {
    guild_id: String,
    channel_id: String,
    subscriptions: Arc<Mutex<HashMap<Event, EventHandlers>>>,
    tx: mpsc::UnboundedSender<(Message, MessageHandler)>,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            client_id: CLIENT_ID.to_string(),
            endpoint: DEFAULT_ENDPOINT.to_string(),
            token_url: TOKEN_URL.to_string(),
            guild_id: None,
            channel_id: None,
        }
    }
}

impl ClientBuilder {
    /// Set the client id to replace the default value
    #[inline]
    pub fn set_client_id<S: AsRef<str>>(mut self, client_id: S) -> Self {
        self.client_id = client_id.as_ref().to_owned();
        self
    }
    /// Set the endpoint to replace the default value
    #[inline]
    pub fn set_endpoint<S: AsRef<str>>(mut self, endpoint: S) -> Self {
        self.endpoint = endpoint.as_ref().to_owned();
        self
    }
    /// Set the url to get access token to replace the default value
    #[inline]
    pub fn set_token_url<S: AsRef<str>>(mut self, token_url: S) -> Self {
        self.token_url = token_url.as_ref().to_owned();
        self
    }
    /// Set the guild id (must)
    #[inline]
    pub fn set_guild_id<S: AsRef<str>>(mut self, guild_id: S) -> Self {
        self.guild_id = Some(guild_id.as_ref().to_owned());
        self
    }
    /// Set the channel id (must)
    #[inline]
    pub fn set_channel_id<S: AsRef<str>>(mut self, channel_id: S) -> Self {
        self.channel_id = Some(channel_id.as_ref().to_owned());
        self
    }

    /// Build the client and get authenticated
    pub async fn build(self) -> Result<Client, ClientBuildError> {
        let url = format!(
            "https://streamkit.kaiheila.cn/overlay/voice/{}/{}",
            self.guild_id
                .as_ref()
                .ok_or(ClientBuildError::MissingField)?,
            self.channel_id
                .as_ref()
                .ok_or(ClientBuildError::MissingField)?
        );
        let url = format!("{}{}", self.endpoint, urlencoding::encode(url.as_str()));
        debug!("source url: {}", url);

        let (tx, mut rx) = mpsc::unbounded_channel::<(Message, MessageHandler)>();

        let mut req = url.into_client_request()?;
        req.headers_mut()
            .insert("Origin", "https://streamkit.kaiheila.cn".parse().unwrap());

        let (mut ws_stream, _) = tokio_tungstenite::connect_async(req).await?;
        // get authorization before all of these
        {
            // ignore first ready message
            let _ = ws_stream.next().await.unwrap()?;

            let authorize_req =
                serde_json::to_string(&Message::authorize_req(&self.client_id)).unwrap(); // should never fail
            ws_stream.send(WsMessage::Text(authorize_req)).await?;
            let response = Message::try_from(ws_stream.next().await.unwrap()?)?;
            let authorize_code = response.get_data_string("code")?;
            debug!("authorize code: {:?}", authorize_code);

            let http_client = reqwest::Client::default();
            let access_token = http_client
                .post(TOKEN_URL)
                .json(&serde_json::json!({
                    "code": authorize_code,
                    "grant_type": "authorization_code",
                    "client_id": CLIENT_ID
                }))
                .send()
                .await?
                .json::<AccessTokenResponse>()
                .await?
                .access_token;
            debug!("access token: {:?}", access_token);

            let authenticate_req =
                serde_json::to_string(&Message::authenticate_req(&self.client_id, access_token))
                    .unwrap(); // should never fail
            ws_stream.send(WsMessage::Text(authenticate_req)).await?;
            let _ = ws_stream.next().await.unwrap()?; // swallow the response
        }

        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        let handlers = Arc::new(Mutex::new(HashMap::new()));
        let broadcast_handlers: Arc<Mutex<HashMap<Event, EventHandlers>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let _handlers = handlers.clone();
        // reading message and send to ws
        tokio::spawn(async move {
            while let Some((msg, handler)) = rx.recv().await {
                {
                    let mut guard = _handlers.lock().await;
                    guard.insert(msg.id.unwrap(), handler);
                    // we want to drop the guard as soon as possible
                }

                let packed = tokio_tungstenite::tungstenite::Message::Text(
                    serde_json::to_string(&msg).unwrap(),
                );
                debug!("sending {:?}", packed);
                if let Err(e) = ws_tx.send(packed).await {
                    debug!("sending task got err: {:?}", e);
                    let mut guard = _handlers.lock().await;
                    let handler = guard.remove(&msg.id.unwrap()).unwrap();
                    handler.send(Err(e.into())).ok();
                }
            }
        });

        let _broadcast_handlers = broadcast_handlers.clone();
        // receiving message and passing it to handler
        tokio::spawn(async move {
            while let Some(ret) = ws_rx.next().await {
                debug!("receiving: {:?}", ret);
                let _handlers = handlers.clone();
                let _broadcast_handlers = _broadcast_handlers.clone();
                tokio::spawn(async move {
                    let ret = ret
                        .and_then(|m| m.into_text())
                        .map_err(ClientError::from)
                        .and_then(|s| {
                            serde_json::from_str::<Message>(s.as_str()).map_err(ClientError::from)
                        });
                    match ret {
                        Ok(msg) => {
                            let mut guard = _handlers.lock().await;
                            match msg.id {
                                None => {
                                    debug_assert_eq!(msg.cmd, Cmd::Dispatch);
                                    debug_assert!(msg.event.is_some());
                                    let key = msg.event.unwrap();
                                    let mut guard = _broadcast_handlers.lock().await;
                                    if let Some(handlers) = guard.get_mut(&key) {
                                        for idx in 0..handlers.len() {
                                            if handlers[idx].send(msg.clone()).is_err() {
                                                // kick out invalid handlers
                                                handlers.remove(idx);
                                            }
                                        }
                                    }
                                }
                                Some(id) => match guard.remove(&id) {
                                    None => error!("unknown message: {:?}", msg),
                                    Some(handler) => {
                                        if let Err(msg) = handler.send(Ok(msg)) {
                                            error!("handler unexpectedly close, message cannot be sent: {:?}", msg)
                                        }
                                    }
                                },
                            }
                        }
                        Err(e) => error!("error receiving message: {:?}", e),
                    }
                });
            }
        });

        Ok(Client {
            guild_id: self.guild_id.unwrap(),
            channel_id: self.channel_id.unwrap(),
            subscriptions: broadcast_handlers,
            tx,
        })
    }
}

impl Client {
    /// Create a default builder
    pub fn new_builder() -> ClientBuilder {
        debug!("initialize default builder");
        ClientBuilder::default()
    }

    /// Send raw [`Message`] call to kaiheila and get response
    pub async fn send_message(&self, message: Message) -> Result<Message, ClientError> {
        debug!("ready to send {:?}", message);
        let (tx, rx) = oneshot::channel();
        self.tx
            .send((message, tx))
            .map_err(|mpsc::error::SendError((msg, _))| ClientError::SenderError(msg))?;
        rx.await?
    }

    /// Subscribe to a kaiheila [`Event`]
    ///
    /// ### returns
    ///
    /// A result, Ok(stream) or Err(clientError)
    pub async fn subscribe(&self, event: Event) -> Result<UnboundedReceiver<Message>, ClientError> {
        debug!("try to subscribe event {:?}", event);

        let ret = self
            .send_message(
                Message::subscribe_builder()
                    .event(event)
                    .guild_id(&self.guild_id)
                    .channel_id(&self.channel_id)
                    .build(),
            )
            .await?; // ignore response
        debug!("subscribe result: {:?}", ret);

        let (tx, rx) = mpsc::unbounded_channel();
        {
            let mut guard = self.subscriptions.lock().await;

            if !guard.contains_key(&event) {
                let entry = guard.entry(event).or_insert_with(Vec::new);
                entry.push(tx);
            } else {
                guard.get_mut(&event).unwrap().push(tx);
            }
        }

        Ok(rx)
    }
}
