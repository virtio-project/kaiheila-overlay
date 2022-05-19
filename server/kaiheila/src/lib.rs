mod message;

#[macro_use]
extern crate log;

use crate::message::Message;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

const CLIENT_ID: &str = "15943749139034";
const DEFAULT_ENDPOINT: &str = "ws://127.0.0.1:5988/?url=";
const TOKEN_URL: &str = "https://www.kaiheila.cn/api/oauth2/token";

#[derive(Debug)]
pub struct ClientBuilder {
    client_id: String,
    endpoint: String,
    token_url: String,
    guild_id: Option<String>,
    channel_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientBuildError {
    #[error("incomplete client builder")]
    MissingField,
    #[error("ws client error: {0}")]
    WsClientError(#[from] tokio_tungstenite::tungstenite::Error),
}

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

pub struct Client {
    tx: mpsc::UnboundedSender<(Message, oneshot::Sender<Result<Message, ClientError>>)>,
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
    pub fn set_client_id(mut self, client_id: String) -> Self {
        self.client_id = client_id;
        self
    }
    pub fn set_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = endpoint;
        self
    }
    pub fn set_token_url(mut self, token_url: String) -> Self {
        self.token_url = token_url;
        self
    }
    pub fn set_guild_id(mut self, guild_id: Option<String>) -> Self {
        self.guild_id = guild_id;
        self
    }
    pub fn set_channel_id(mut self, channel_id: Option<String>) -> Self {
        self.channel_id = channel_id;
        self
    }

    pub async fn build(self) -> Result<Client, ClientBuildError> {
        let url = format!(
            "https://streamkit.kaiheila.cn/overlay/voice/{}/{}",
            self.guild_id.ok_or(ClientBuildError::MissingField)?,
            self.channel_id.ok_or(ClientBuildError::MissingField)?
        );
        let url = format!("{}{}", self.endpoint, urlencoding::encode(url.as_str()));

        let (tx, mut rx) =
            mpsc::unbounded_channel::<(Message, oneshot::Sender<Result<Message, ClientError>>)>();

        let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;
        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        let handlers = Arc::new(Mutex::new(HashMap::new()));

        let _handlers = handlers.clone();
        // reading message and send to ws
        tokio::spawn(async move {
            while let Some((msg, handler)) = rx.recv().await {
                {
                    let mut guard = _handlers.lock().await;
                    guard.insert(msg.id, handler);
                    // we want to drop the guard as soon as possible
                }

                let packed = tokio_tungstenite::tungstenite::Message::Text(
                    serde_json::to_string(&msg).unwrap(),
                );
                if let Err(e) = ws_tx.send(packed).await {
                    let mut guard = _handlers.lock().await;
                    let handler = guard.remove(&msg.id).unwrap();
                    handler.send(Err(e.into())).ok();
                }
            }
        });

        // receiving message and passing it to handler
        tokio::spawn(async move {
            while let Some(ret) = ws_rx.next().await {
                let _handlers = handlers.clone();
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
                            match guard.remove(&msg.id) {
                                None => error!("unknown message: {:?}", msg),
                                Some(handler) => {
                                    if let Err(msg) = handler.send(Ok(msg)) {
                                        error!("handler unexpectedly close, message cannot be sent: {:?}", msg)
                                    }
                                }
                            }
                        }
                        Err(e) => error!("error receiving message: {:?}", e),
                    }
                });
            }
        });

        Ok(Client { tx })
    }
}

impl Client {
    pub fn new_builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    pub async fn send_message(&self, message: Message) -> Result<Message, ClientError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send((message, tx))
            .map_err(|mpsc::error::SendError((msg, _))| ClientError::SenderError(msg))?;
        rx.await?
    }
}
