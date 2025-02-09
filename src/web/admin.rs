use aide::{
    openapi::{
        HeaderStyle, Parameter, ParameterData, ParameterSchemaOrContent, ReferenceOr, SchemaObject,
    },
    transform::TransformOperation,
};
use axum::extract::Query;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use reqwest::StatusCode;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;
use tracing::info;

use crate::db::{check_users_exist, search_streams, search_user_logins};
use crate::web::schema::{ChannelParam, Streams, UserHasLogs, UserLogins, UserParam};
use crate::{app::App, bot::BotMessage, error::Error};

pub async fn admin_auth(
    app: State<App>,
    request: Request,
    next: Next,
) -> Result<Response, impl IntoResponse> {
    if let Some(admin_key) = &app.config.admin_api_key {
        if request
            .headers()
            .get("X-Api-Key")
            .and_then(|value| value.to_str().ok())
            == Some(admin_key)
        {
            let response = next.run(request).await;
            return Ok(response);
        }
    }

    Err((StatusCode::FORBIDDEN, "No, I don't think so"))
}

pub fn admin_auth_doc(op: &mut TransformOperation) {
    let schema = aide::generate::in_context(|ctx| ctx.schema.subschema_for::<String>());

    op.inner_mut()
        .parameters
        .push(ReferenceOr::Item(Parameter::Header {
            parameter_data: ParameterData {
                name: "X-Api-Key".to_owned(),
                description: Some("Configured admin API key".to_owned()),
                required: true,
                deprecated: None,
                format: ParameterSchemaOrContent::Schema(SchemaObject {
                    json_schema: schema,
                    external_docs: None,
                    example: None,
                }),
                example: None,
                examples: Default::default(),
                explode: None,
                extensions: Default::default(),
            },
            style: HeaderStyle::Simple,
        }));
}

#[derive(Deserialize, JsonSchema)]
pub struct ChannelsRequest {
    /// List of channel ids
    pub channels: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UsersRequest {
    /// Channel id
    pub channel: String,
    /// List of user ids
    pub users: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UserLoginsRequest {
    /// The user
    #[serde(flatten)]
    pub user: UserParam,
}

#[derive(Deserialize, JsonSchema)]
pub struct StreamsRequest {
    /// The channel id
    #[serde(flatten)]
    pub channel: ChannelParam,
    /// The offset
    pub offset: Option<u64>,
}

pub async fn add_channels(
    Extension(bot_tx): Extension<Sender<BotMessage>>,
    app: State<App>,
    Json(ChannelsRequest { channels }): Json<ChannelsRequest>,
) -> Result<(), Error> {
    let users = app.get_users(channels, vec![], false).await?;
    let names = users.into_values().collect();

    bot_tx.send(BotMessage::JoinChannels(names)).await.unwrap();

    Ok(())
}

pub async fn remove_channels(
    Extension(bot_tx): Extension<Sender<BotMessage>>,
    app: State<App>,
    Json(ChannelsRequest { channels }): Json<ChannelsRequest>,
) -> Result<(), Error> {
    let users = app.get_users(channels, vec![], false).await?;
    let names = users.into_values().collect();

    bot_tx.send(BotMessage::PartChannels(names)).await.unwrap();

    Ok(())
}

pub async fn check_users_existence(
    app: State<App>,
    Json(UsersRequest { channel, users }): Json<UsersRequest>,
) -> Result<Json<Vec<UserHasLogs>>, Error> {
    let users = check_users_exist(&app.db, &channel, &users).await?;
    Ok(Json(users))
}

pub async fn find_user_logins(
    app: State<App>,
    Query(UserLoginsRequest { user }): Query<UserLoginsRequest>,
) -> Result<Json<UserLogins>, Error> {
    let logins = search_user_logins(&app, &user).await?;
    Ok(Json(logins))
}

pub async fn find_streams(
    app: State<App>,
    Query(StreamsRequest { channel, offset }): Query<StreamsRequest>,
) -> Result<Json<Streams>, Error> {
    let channel_id = match channel {
        ChannelParam::ChannelId(id) => id,
        ChannelParam::Channel(name) => app.get_user_id_by_name(&name).await?,
    };
    
    let streams = search_streams(&app.db, &channel_id, offset.unwrap_or(0)).await?;
    Ok(Json(streams))
}