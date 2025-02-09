use std::fmt::Display;
use strum::Display;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use crate::db::schema::Stream;
use super::responders::logs::{JsonResponseType, LogsResponseType};

#[derive(Serialize, JsonSchema)]
pub struct ChannelsList {
    pub channels: Vec<Channel>,
}

#[derive(Serialize, JsonSchema)]
pub struct Channel {
    pub name: String,
    #[serde(rename = "userID")]
    pub user_id: String,
}

#[derive(Debug, Deserialize, JsonSchema, Display)]
pub enum ChannelIdType {
    #[serde(rename = "channel")]
    #[strum(serialize = "channel")]
    Name,
    #[serde(rename = "channelid")]
    #[strum(serialize = "channelid")]
    Id,
}

#[derive(Debug, Deserialize, JsonSchema, Display)]
pub enum UserIdType {
    #[serde(rename = "user")]
    #[strum(serialize = "user")]
    Name,
    #[serde(rename = "userid")]
    #[strum(serialize = "userid")]
    Id,
}

#[derive(Deserialize, JsonSchema)]
pub struct UserLogsDatePath {
    pub year: String,
    pub month: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ChannelLogsByDatePath {
    #[serde(flatten)]
    pub channel_info: LogsPathChannel,
    #[serde(flatten)]
    pub date: LogsPathDate,
}

#[derive(Deserialize, JsonSchema)]
pub struct LogsPathDate {
    pub year: String,
    pub month: String,
    pub day: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct LogsPathChannel {
    pub channel_id_type: ChannelIdType,
    pub channel: String,
}

#[derive(Deserialize, Debug, JsonSchema, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct LogsParams {
    #[serde(default, deserialize_with = "deserialize_bool_param")]
    pub json: bool,
    #[serde(default, deserialize_with = "deserialize_bool_param")]
    pub json_basic: bool,
    #[serde(default, deserialize_with = "deserialize_bool_param")]
    pub raw: bool,
    #[serde(default, deserialize_with = "deserialize_bool_param")]
    pub reverse: bool,
    #[serde(default, deserialize_with = "deserialize_bool_param")]
    pub ndjson: bool,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

impl LogsParams {
    pub fn response_type(&self) -> LogsResponseType {
        if self.raw {
            LogsResponseType::Raw
        } else if self.json_basic {
            LogsResponseType::Json(JsonResponseType::Basic)
        } else if self.json {
            LogsResponseType::Json(JsonResponseType::Full)
        } else if self.ndjson {
            LogsResponseType::NdJson
        } else {
            LogsResponseType::Text
        }
    }
}

fn deserialize_bool_param<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<&str>::deserialize(deserializer)?.is_some())
}

#[derive(Deserialize, Debug, JsonSchema)]
pub struct SearchParams {
    pub q: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AvailableLogs {
    pub available_logs: Vec<AvailableLogDate>,
}

#[derive(Serialize, JsonSchema)]
pub struct AvailableLogDate {
    pub year: String,
    pub month: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<String>,
}

impl Display for AvailableLogDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.year, self.month)?;

        if let Some(day) = &self.day {
            write!(f, "/{day}")?;
        }

        Ok(())
    }
}

#[derive(Serialize, JsonSchema)]
pub struct UserHasLogs {
    /// User ID
    pub user: String,
    /// Whether the user has logs in the channel
    pub has_logs: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct AssignedStream {
    /// Stream ID
    pub id: String,
    /// Start time unix epoch
    pub started_at: u32,
}

impl From<Stream> for AssignedStream {
    fn from(stream: Stream) -> Self {
        Self {
            id: stream.stream_id,
            started_at: stream.started_at,
        }
    }
}

#[derive(Serialize, JsonSchema)]
pub struct Streams {
    /// Channel ID
    pub channel_id: String,
    /// Total number of streams
    pub total: u64,
    /// List of known streams
    pub streams: Vec<AssignedStream>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AvailableLogsParams {
    #[serde(flatten)]
    pub channel: ChannelParam,
    #[serde(flatten)]
    pub user: Option<UserParam>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum UserParam {
    User(String),
    UserId(String),
}

impl AsRef<str> for UserParam {
    fn as_ref(&self) -> &str {
        match self {
            UserParam::User(user) => user,
            UserParam::UserId(user_id) => user_id,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChannelParam {
    Channel(String),
    ChannelId(String),
}

#[derive(Deserialize, JsonSchema)]
pub struct UserLogPathParams {
    pub channel_id_type: ChannelIdType,
    pub channel: String,
    pub user_id_type: UserIdType,
    pub user: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelLogsStats {
    pub message_count: u64,
    pub top_chatters: Vec<UserLogsStats>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserLogsStats {
    pub user_id: String,
    pub message_count: u64,
}

#[derive(Deserialize, JsonSchema)]
pub struct UserNameHistoryParam {
    pub user_id: String,
}

#[derive(Serialize, JsonSchema)]
pub struct PreviousName {
    pub user_login: String,
    #[schemars(with = "String")]
    pub last_timestamp: DateTime<Utc>,
    #[schemars(with = "String")]
    pub first_timestamp: DateTime<Utc>,
}
