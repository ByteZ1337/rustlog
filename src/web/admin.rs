use aide::{
    openapi::{
        HeaderStyle, Parameter, ParameterData, ParameterSchemaOrContent, ReferenceOr, SchemaObject,
    },
    transform::TransformOperation,
};
use axum::{
    Extension,
    extract::{Request, State},
    Json,
    middleware::Next, response::{IntoResponse, Response},
};
use axum::extract::Query;
use reqwest::StatusCode;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;
use tracing::info;

use crate::{app::App, bot::BotMessage, error::Error};
use crate::db::{check_users_exist, search_user_logins};
use crate::web::schema::{UserHasLogs, UserLogins, UserParam};

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
    let schema = aide::gen::in_context(|ctx| ctx.schema.subschema_for::<String>());

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