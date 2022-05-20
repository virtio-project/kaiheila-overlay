use std::collections::HashMap;

use crate::WsMessage;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Message related errors
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("transport error: {0}")]
    TransportError(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("deserialization failed: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("request resources not found")]
    NotFound,
}

/// description of a possible kaiheila message
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    pub(crate) cmd: Cmd,
    #[serde(rename = "evt", skip_serializing_if = "Option::is_none")]
    pub(crate) event: Option<Event>,
}

/// command of a kaiheila [`Message`]
#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Cmd {
    Authenticate,
    Authorize,
    CreateChannelInvite,
    Dispatch,
    GetChannel,
    GetChannelList,
    GetGuildList,
    ObsVoiceChange,
    Subscribe,
}

/// event of a kaiheila [`Message`]
#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Ready,
    AudioChannelUserChange,
    AudioChannelUserTalk,
    AudioChannelMicHeadersetStatus,
    GuildStatus,
    MessageCreate,
    MessageUpdate,
    MessageDelete,
}

/// builder of a subscription command
#[derive(Default, Clone, Debug)]
pub struct SubscribeMessageBuilder {
    args: HashMap<String, serde_json::Value>,
    event: Option<Event>,
}

/// description of a kaiheila access token response
#[derive(Clone, Debug, Deserialize)]
pub struct AccessTokenResponse {
    pub access_token: String,
    pub expire_in: u32,
    pub token_type: String,
    pub scope: String,
}

impl TryFrom<WsMessage> for Message {
    type Error = MessageError;

    fn try_from(msg: WsMessage) -> Result<Self, Self::Error> {
        Ok(serde_json::from_str(msg.to_text()?)?)
    }
}

impl Message {
    /// helper method to get a value by key from args
    pub fn get_args_string<K: AsRef<str>>(&self, k: K) -> Result<String, MessageError> {
        match self.args {
            None => Err(MessageError::NotFound),
            Some(ref m) => m
                .get(k.as_ref())
                .ok_or(MessageError::NotFound)
                .and_then(|v| v.as_str().ok_or(MessageError::NotFound).map(str::to_string)),
        }
    }

    /// helper method to get a value by key from data
    ///
    /// ### note
    ///
    /// if the `data` is not a [`serde_json::Map`], you will get an error
    pub fn get_data_string<K: AsRef<str>>(&self, k: K) -> Result<String, MessageError> {
        match self.data.as_ref().and_then(|d| d.as_object()) {
            None => Err(MessageError::NotFound),
            Some(m) => m
                .get(k.as_ref())
                .ok_or(MessageError::NotFound)
                .and_then(|v| v.as_str().ok_or(MessageError::NotFound).map(str::to_string)),
        }
    }

    /// treat the data as an array (if possible)
    pub fn get_data_array(&self) -> Option<&Vec<Value>> {
        self.data.as_ref().and_then(|d| d.as_array())
    }

    /// creates an authorize_req
    pub fn authorize_req<C: AsRef<str>>(client_id: C) -> Self {
        let mut rng = rand::thread_rng();
        Message {
            id: Some(rng.gen_range(1000000..9999999)),
            args: Some(HashMap::from_iter([
                (
                    "client_id".to_string(),
                    serde_json::Value::String(client_id.as_ref().to_string()),
                ),
                (
                    "scopes".to_string(),
                    serde_json::Value::Array(vec![
                        serde_json::Value::String("rpc".to_string()),
                        serde_json::Value::String("get_guild_info".to_string()),
                    ]),
                ),
                (
                    "prompt".to_string(),
                    serde_json::Value::String("none".to_string()),
                ),
            ])),
            data: None,
            cmd: Cmd::Authorize,
            event: None,
        }
    }

    /// creates an authenticate_req
    pub fn authenticate_req<C: AsRef<str>>(client_id: C, token: String) -> Self {
        let mut rng = rand::thread_rng();
        Message {
            id: Some(rng.gen_range(1000000..9999999)),
            args: Some(HashMap::from_iter([
                (
                    "client_id".to_string(),
                    serde_json::Value::String(client_id.as_ref().to_string()),
                ),
                ("token".to_string(), serde_json::Value::String(token)),
            ])),
            data: None,
            cmd: Cmd::Authenticate,
            event: None,
        }
    }

    // {"id":1216596,"args":{"channel_id":"1714016194916588","guild_id":"1561035437838649"},"cmd":"subscribe","evt":"audio_channel_user_change"}
    // {"id":9599219,"args":{"channel_id":"1714016194916588"},"cmd":"subscribe","evt":"audio_channel_user_talk"}
    // {"id":7153765,"args":{"channel_id":"1714016194916588"},"cmd":"subscribe","evt":"audio_channel_mic_headerset_status"}
    pub fn subscribe_builder() -> SubscribeMessageBuilder {
        SubscribeMessageBuilder::default()
    }
}

impl SubscribeMessageBuilder {
    pub fn channel_id<C: AsRef<str>>(mut self, channel_id: C) -> Self {
        self.args.insert(
            "channel_id".to_string(),
            serde_json::Value::String(channel_id.as_ref().to_string()),
        );
        self
    }

    pub fn guild_id<G: AsRef<str>>(mut self, guild_id: G) -> Self {
        self.args.insert(
            "guild_id".to_string(),
            serde_json::Value::String(guild_id.as_ref().to_string()),
        );
        self
    }

    pub fn event(mut self, event: Event) -> Self {
        self.event = Some(event);
        self
    }

    pub fn build(self) -> Message {
        let mut rng = rand::thread_rng();
        let Self { args, event } = self;
        Message {
            id: Some(rng.gen_range(1000000..9999999)),
            args: Some(args),
            data: None,
            cmd: Cmd::Subscribe,
            event,
        }
    }
}
