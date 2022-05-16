use std::collections::HashMap;
use actix_web_actors::ws::Frame;
use awc::ws;
use bytestring::ByteString;
use futures_util::{SinkExt, Stream, StreamExt};
use rand::Rng;
use tokio::sync::broadcast;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const CLIENT_ID: &str = "15943749139034";
const API: &str = "ws://127.0.0.1:5988/?url=";
const TOKEN_URL: &str = "https://www.kaiheila.cn/api/oauth2/token";

#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("unsupported message")]
    UnsupportedMessage,
    #[error("deserialization failed: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("request resources not found")]
    NotFound,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("websocket error: {0}")]
    WsError(#[from] awc::error::WsClientError),
    #[error("ws protocol error: {0}")]
    ProtocolError(#[from] actix_web_actors::ws::ProtocolError),
    #[error("json ser/de failed: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("message error: {0}")]
    MessageError(#[from] MessageError),
    #[error("awc client error: {0}")]
    ClientError(#[from] awc::error::SendRequestError),
    #[error("awc json error: {0}")]
    AwcJsonError(#[from] awc::error::JsonPayloadError)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<HashMap<String, serde_json::Value>>,
    cmd: Cmd,
    #[serde(rename = "evt", skip_serializing_if = "Option::is_none")]
    event: Option<Event>,
}

#[derive(Default, Clone, Debug)]
pub struct SubscribeMessageBuilder {
    args: HashMap<String, serde_json::Value>,
    event: Option<Event>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Cmd {
    Authenticate,
    Authorize,
    CreateChannelInvite,
    GetChannel,
    GetChannelList,
    GetGuildList,
    ObsVoiceChange,
    Subscribe,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Event {
    AudioChannelUserChange,
    AudioChannelUserTalk,
    AudioChannelMicHeadersetStatus,
    GuildStatus,
    MessageCreate,
    MessageUpdate,
    MessageDelete,
}

#[derive(Clone, Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    expires_in: u32,
    refresh_token: String,
}

async fn connect(guild_id: &str, channel_id: &str) -> Result<(), ClientError> {
    let url = format!("https://streamkit.kaiheila.cn/overlay/voice/{}/{}", guild_id, channel_id);
    let url = format!("{}{}", API, urlencoding::encode(url.as_str()));
    let client = awc::Client::new();
    let (res, mut ws) = client
        .ws(url)
        .origin("https://streamkit.kaiheila.cn")
        .protocols(["ws_streamkit"])
        .connect()
        .await?;
    debug!("{:?}", res);
    let _ = ws.next().await.unwrap()?;

    let authorize_req = serde_json::to_string(&Message::authorize_req())?;
    ws.send(ws::Message::Text(ByteString::from(authorize_req))).await?;

    let frame = ws.next().await.unwrap()?;
    let authorize_code = Message::try_from(frame)?.get_data_string("code")?;

    let resp = client.post(TOKEN_URL)
        .send_json(&serde_json::json!({
            "code": authorize_code,
            "grant_type": "authorization_code",
            "client_id": CLIENT_ID
        }))
        .await?
        .json::<AccessTokenResponse>()
        .await?;
    let access_token = resp.access_token;

    let authenticate_req = serde_json::to_string(&Message::authenticate_req(token))?;
    ws.send(ws::Message::Text(ByteString::from(authenticate_req))).await?;
    let _ = ws.next().await.unwrap()?;

    let sub_user_change = Message::subscribe_builder()
        .channel_id(channel_id)
        .guild_id(guild_id)
        .event(Event::AudioChannelUserChange)
        .build();
    let sub_user_talk = Message::subscribe_builder()
        .channel_id(channel_id)
        .event(Event::AudioChannelUserTalk)
        .build();

    Ok(())
}

impl TryFrom<ws::Frame> for Message {
    type Error = MessageError;

    fn try_from(frame: Frame) -> Result<Self, Self::Error> {
        match frame {
            ws::Frame::Text(content) => Ok(serde_json::from_slice::<Message>(content.as_ref())?),
            _ => Err(MessageError::UnsupportedMessage)
        }
    }
}

impl Message {
    pub fn get_args_string<K: AsRef<str>>(&self, k: K) -> Result<String, MessageError> {
        return Message::get_string_inner(&self.args, k)
    }

    pub fn get_data_string<K: AsRef<str>>(&self, k: K) -> Result<String, MessageError> {
        return Message::get_string_inner(&self.data, k)
    }

    fn get_string_inner<K: AsRef<str>>(map: &Option<HashMap<String, serde_json::Value>>, k: K) -> Result<String, MessageError> {
        match map {
            None => Err(MessageError::NotFound),
            Some(m) => m.get(k.as_ref())
                .ok_or(MessageError::NotFound)
                .and_then(|v| v.as_str().ok_or(MessageError::NotFound).map(str::to_string))
        }
    }

    pub fn authorize_req() -> Self {
        let mut rng = rand::thread_rng();
        Message {
            id: rng.gen_range(1000000..9999999),
            args: Some(HashMap::from_iter([
                ("client_id".to_string(), serde_json::Value::String(CLIENT_ID.to_string())),
                ("scopes".to_string(), serde_json::Value::Array(vec![
                    serde_json::Value::String("rpc".to_string()),
                    serde_json::Value::String("get_guild_info".to_string())
                ])),
                ("prompt".to_string(), serde_json::Value::String("none".to_string())),
            ])),
            data: None,
            cmd: Cmd::Authorize,
            event: None
        }
    }

    pub fn authenticate_req(token: String) -> Self {
        let mut rng = rand::thread_rng();
        Message {
            id: rng.gen_range(1000000..9999999),
            args: Some(HashMap::from_iter([
                ("client_id".to_string(), serde_json::Value::String(CLIENT_ID.to_string())),
                ("token".to_string(), serde_json::Value::String(token))
            ])),
            data: None,
            cmd: Cmd::Authenticate,
            event: None
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
            serde_json::Value::String(channel_id.as_ref().to_string())
        );
        self
    }

    pub fn guild_id<G: AsRef<str>>(mut self, guild_id: G) -> Self {
        self.args.insert(
            "guild_id".to_string(),
            serde_json::Value::String(guild_id.as_ref().to_string())
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
            id: rng.gen_range(1000000..9999999),
            args: Some(args),
            data: None,
            cmd: Cmd::Subscribe,
            event
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[actix_rt::test]
    async fn test_connect() {
        connect("1561035437838649", "1714016194916588").await;
    }
}