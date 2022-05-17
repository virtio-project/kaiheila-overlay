use std::collections::HashMap;

use serde::{Serialize, Deserialize};

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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmd {
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
pub enum Event {
    AudioChannelUserChange,
    AudioChannelUserTalk,
    AudioChannelMicHeadersetStatus,
    GuildStatus,
    MessageCreate,
    MessageUpdate,
    MessageDelete,
}