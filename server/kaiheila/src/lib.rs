mod message;

#[macro_use] extern crate log;

use futures_util::StreamExt;

const CLIENT_ID: &str = "15943749139034";
const DEFAULT_ENDPOINT: &str = "ws://127.0.0.1:5988/?url=";
const TOKEN_URL: &str = "https://www.kaiheila.cn/api/oauth2/token";

type WsStream = actix_codec::Framed<awc::BoxedSocket, actix_http::ws::Codec>;

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
    #[error("awc ws client error: {0}")]
    WsClientError(#[from] awc::error::WsClientError),
    #[error("ws protocol error: {0}")]
    WsProtocolError(#[from] actix_http::ws::ProtocolError),
}

pub struct Client {
    client: awc::Client,
    stream: WsStream,
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
        let client = awc::Client::new();
        let (res, mut ws) = client
            .ws(url)
            .origin("https://streamkit.kaiheila.cn")
            .protocols(["ws_streamkit"])
            .connect()
            .await?;
        debug!("{:?}", res);
        let _ = ws.next().await.unwrap()?;

        Ok(Client {
            client,
            stream: ws
        })
    }
}

impl Client {
    pub fn new_builder() -> ClientBuilder {
        ClientBuilder::default()
    }
}

