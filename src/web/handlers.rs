use std::time::Duration;

use super::{
    responders::logs::LogsResponse,
    schema::{
        AvailableLogs, AvailableLogsParams, Channel, ChannelIdType, ChannelLogsByDatePath,
        ChannelLogsStats, ChannelParam, ChannelsList, LogsParams, LogsPathChannel, SearchParams,
        UserLogPathParams, UserLogsStats, UserNameHistoryParam, UserParam
    },
};
use aide::axum::IntoApiResponse;
use axum::{
    extract::{Path, Query, RawQuery, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use axum_extra::{headers::CacheControl, TypedHeader};
use chrono::{DateTime, Days, Months, NaiveDate, NaiveTime, Utc};

use crate::web::schema::{UserIdType, UserLogsDatePath};
use crate::{
    app::App,
    db::{
        self, read_available_channel_logs, read_available_user_logs, read_channel,
        read_random_channel_line, read_random_user_line, read_user,
    },
    error::Error,
    logs::{schema::LogRangeParams, stream::LogsStream},
    web::schema::LogsPathDate,
    Result,
};

// can't have this as a layer because the user logins and ids are passed in a lot of different ways
pub fn check_logs_auth(
    app: &State<App>,
    headers: HeaderMap,
    user: Option<&str>,
) -> Result<()> {
    if matches!(user, Some(user) if user == "gofishgame" || user == "951349582") {
        return Ok(());
    }

    if let Some(admin_key) = &app.config.admin_api_key {
        if headers
            .get("X-Api-Key")
            .and_then(|value| value.to_str().ok())
            == Some(admin_key)
        {
            return Ok(());
        }
    }

    Err(Error::InvalidAuthKey)
}

pub async fn get_channels(
    app: State<App>,
    headers: HeaderMap,
) -> Result<Response> {
    check_logs_auth(&app, headers, None)?;
    let channel_ids = app.config.channels.read().unwrap().clone();

    let channels = app
        .get_users(Vec::from_iter(channel_ids), vec![], false)
        .await
        .unwrap();

    let json = Json(ChannelsList {
        channels: channels
            .into_iter()
            .map(|(user_id, name)| Channel { name, user_id })
            .collect(),
    });
    Ok((cache_header(600), json).into_response())
}

pub async fn get_channel_logs(
    Path(LogsPathChannel {
        channel_id_type,
        channel,
    }): Path<LogsPathChannel>,
    Query(range_params): Query<LogRangeParams>,
    Query(logs_params): Query<LogsParams>,
    RawQuery(query): RawQuery,
    app: State<App>,
    headers: HeaderMap,
) -> Result<Response> {
    check_logs_auth(&app, headers, None)?;

    let channel_id = match channel_id_type {
        ChannelIdType::Name => app.get_user_id_by_name(&channel).await?,
        ChannelIdType::Id => channel.clone(),
    };

    if let Some(range) = range_params.range() {
        let logs = get_channel_logs_inner(&app, &channel_id, logs_params, range).await?;
        Ok(logs.into_response())
    } else {
        let available_logs = read_available_channel_logs(&app.db, &channel_id).await?;
        let latest_log = available_logs.first().ok_or(Error::NotFound)?;

        let mut new_uri = format!("/{channel_id_type}/{channel}/{latest_log}");
        if let Some(query) = query {
            new_uri.push('?');
            new_uri.push_str(&query);
        }

        Ok(Redirect::to(&new_uri).into_response())
    }
}

pub async fn get_channel_stats(
    Path(LogsPathChannel {
        channel_id_type,
        channel,
    }): Path<LogsPathChannel>,
    Query(range_params): Query<LogRangeParams>,
    app: State<App>,
    headers: HeaderMap,
) -> Result<Json<ChannelLogsStats>> {
    check_logs_auth(&app, headers, None)?;
    let channel_id = match channel_id_type {
        ChannelIdType::Name => app.get_user_id_by_name(&channel).await?,
        ChannelIdType::Id => channel.clone(),
    };
    let stats = db::get_channel_stats(&app.db, &channel_id, range_params).await?;

    Ok(Json(stats))
}

pub async fn get_user_stats(
    Path(user_params): Path<UserLogPathParams>,
    Query(range_params): Query<LogRangeParams>,
    app: State<App>,
    headers: HeaderMap,
) -> Result<Json<UserLogsStats>> {
    let (channel_id, user_id) = resolve_user_params(&user_params, &app).await?;
    check_logs_auth(&app, headers, Some(&user_id))?;

    app.check_opted_out(&channel_id, Some(&user_id))?;

    let stats = db::get_user_stats(&app.db, &channel_id, &user_id, range_params).await?;

    Ok(Json(stats))
}

pub async fn get_channel_logs_by_date(
    app: State<App>,
    Path(channel_log_params): Path<ChannelLogsByDatePath>,
    Query(logs_params): Query<LogsParams>,
    headers: HeaderMap,
) -> Result<impl IntoApiResponse> {
    check_logs_auth(&app, headers, None)?;

    let channel_id = match channel_log_params.channel_info.channel_id_type {
        ChannelIdType::Name => {
            app.get_user_id_by_name(&channel_log_params.channel_info.channel)
                .await?
        }
        ChannelIdType::Id => channel_log_params.channel_info.channel.clone(),
    };

    let LogsPathDate { year, month, day } = channel_log_params.date;

    let from = NaiveDate::from_ymd_opt(year.parse()?, month.parse()?, day.parse()?)
        .ok_or_else(|| Error::InvalidParam("Invalid date".to_owned()))?
        .and_time(NaiveTime::default())
        .and_utc();
    let to = from
        .checked_add_days(Days::new(1))
        .ok_or_else(|| Error::InvalidParam("Date out of range".to_owned()))?;

    get_channel_logs_inner(&app, &channel_id, logs_params, (from, to)).await
}

async fn get_channel_logs_inner(
    app: &App,
    channel_id: &str,
    params: LogsParams,
    range: (DateTime<Utc>, DateTime<Utc>),
) -> Result<impl IntoApiResponse> {
    app.check_opted_out(channel_id, None)?;

    let stream = read_channel(&app.db, channel_id, params, &app.flush_buffer, range).await?;

    let logs = LogsResponse {
        response_type: params.response_type(),
        stream,
    };

    let cache = if Utc::now() < range.1 {
        no_cache_header()
    } else {
        cache_header(36000)
    };

    Ok((cache, logs))
}

pub async fn get_user_logs(
    Path(user_params): Path<UserLogPathParams>,
    Query(range_params): Query<LogRangeParams>,
    Query(logs_params): Query<LogsParams>,
    RawQuery(query): RawQuery,
    app: State<App>,
    headers: HeaderMap,
) -> Result<impl IntoApiResponse> {
    let (channel_id, user_id) = resolve_user_params(&user_params, &app).await?;
    check_logs_auth(&app, headers, Some(&user_id))?;

    app.check_opted_out(&channel_id, Some(&user_id))?;

    if let Some(range) = range_params.range() {
        let logs = get_user_logs_inner(&app, &channel_id, &user_id, logs_params, range).await?;
        Ok(logs.into_response())
    } else {
        let available_logs = read_available_user_logs(&app.db, &channel_id, &user_id).await?;
        let latest_log = available_logs.first().ok_or(Error::NotFound)?;

        let UserLogPathParams {
            channel_id_type,
            channel,
            user_id_type,
            user,
        } = user_params;

        let mut new_uri =
            format!("/{channel_id_type}/{channel}/{user_id_type}/{user}/{latest_log}");
        if let Some(query) = query {
            new_uri.push('?');
            new_uri.push_str(&query);
        }
        Ok(Redirect::to(&new_uri).into_response())
    }
}

pub async fn get_user_logs_by_date(
    app: State<App>,
    Path(user_params): Path<UserLogPathParams>,
    Path(user_logs_date): Path<UserLogsDatePath>,
    Query(logs_params): Query<LogsParams>,
    headers: HeaderMap,
) -> Result<impl IntoApiResponse> {
    let (channel_id, user_id) = resolve_user_params(&user_params, &app).await?;
    check_logs_auth(&app, headers, Some(&user_id))?;

    app.check_opted_out(&channel_id, Some(&user_id))?;

    let year = user_logs_date.year.parse()?;
    let month = user_logs_date.month.parse()?;

    let from = NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| Error::InvalidParam("Invalid date".to_owned()))?
        .and_time(NaiveTime::default())
        .and_utc();
    let to = from
        .checked_add_months(Months::new(1))
        .ok_or_else(|| Error::InvalidParam("Date out of range".to_owned()))?;

    get_user_logs_inner(&app, &channel_id, &user_id, logs_params, (from, to)).await
}

async fn get_user_logs_inner(
    app: &App,
    channel_id: &str,
    user_id: &str,
    logs_params: LogsParams,
    range: (DateTime<Utc>, DateTime<Utc>),
) -> Result<impl IntoApiResponse> {
    let stream = read_user(
        &app.db,
        channel_id,
        user_id,
        logs_params,
        &app.flush_buffer,
        range,
    )
    .await?;

    let logs = LogsResponse {
        stream,
        response_type: logs_params.response_type(),
    };

    let cache = if Utc::now() < range.1 {
        no_cache_header()
    } else {
        cache_header(36000)
    };

    Ok((cache, logs))
}

pub async fn list_available_logs(
    Query(AvailableLogsParams { user, channel }): Query<AvailableLogsParams>,
    app: State<App>,
    headers: HeaderMap,
) -> Result<impl IntoApiResponse> {
    check_logs_auth(&app, headers, user.as_ref().map(|u| u.as_ref()))?;

    let channel_id = match channel {
        ChannelParam::ChannelId(id) => id,
        ChannelParam::Channel(name) => app.get_user_id_by_name(&name).await?,
    };

    let available_logs = if let Some(user) = user {
        let user_id = match user {
            UserParam::UserId(id) => id,
            UserParam::User(name) => app.get_user_id_by_name(&name).await?,
        };
        app.check_opted_out(&channel_id, Some(&user_id))?;
        read_available_user_logs(&app.db, &channel_id, &user_id).await?
    } else {
        return Err(Error::NotFound);
        // app.check_opted_out(&channel_id, None)?;
        // read_available_channel_logs(&app.db, &channel_id).await?
    };

    if !available_logs.is_empty() {
        Ok((cache_header(600), Json(AvailableLogs { available_logs })))
    } else {
        Err(Error::NotFound)
    }
}

pub async fn random_channel_line(
    app: State<App>,
    headers: HeaderMap,
    Path(LogsPathChannel {
        channel_id_type,
        channel,
    }): Path<LogsPathChannel>,
    Query(logs_params): Query<LogsParams>,
) -> Result<impl IntoApiResponse> {
    check_logs_auth(&app, headers, None)?;

    let channel_id = match channel_id_type {
        ChannelIdType::Name => app.get_user_id_by_name(&channel).await?,
        ChannelIdType::Id => channel,
    };

    let random_line = read_random_channel_line(&app.db, &channel_id).await?;
    let stream = LogsStream::new_provided(vec![random_line])?;

    let logs = LogsResponse {
        stream,
        response_type: logs_params.response_type(),
    };
    Ok((no_cache_header(), logs))
}

pub async fn random_user_line(
    app: State<App>,
    Path(user_params): Path<UserLogPathParams>,
    Query(logs_params): Query<LogsParams>,
    headers: HeaderMap,
) -> Result<impl IntoApiResponse> {
    let (channel_id, user_id) = resolve_user_params(&user_params, &app).await?;
    check_logs_auth(&app, headers, Some(&user_id))?;

    app.check_opted_out(&channel_id, Some(&user_id))?;

    let random_line = read_random_user_line(&app.db, &channel_id, &user_id).await?;
    let stream = LogsStream::new_provided(vec![random_line])?;

    let logs = LogsResponse {
        stream,
        response_type: logs_params.response_type(),
    };
    Ok((no_cache_header(), logs))
}

pub async fn optout(_app: State<App>) -> Json<String> {
    Json("No, I don't think so".to_owned())
}

pub async fn search_user_logs(
    app: State<App>,
    Path(user_params): Path<UserLogPathParams>,
    Query(search_params): Query<SearchParams>,
    Query(logs_params): Query<LogsParams>,
    headers: HeaderMap,
) -> Result<impl IntoApiResponse> {
    let (channel_id, user_id) = resolve_user_params(&user_params, &app).await?;
    check_logs_auth(&app, headers, Some(&user_id))?;

    app.check_opted_out(&channel_id, Some(&user_id))?;

    let stream = db::search_user_logs(
        &app.db,
        &channel_id,
        &user_id,
        &search_params.q,
        logs_params,
    )
    .await?;

    let logs = LogsResponse {
        stream,
        response_type: logs_params.response_type(),
    };
    Ok(logs)
}

pub async fn get_user_name_history(
    app: State<App>,
    Path(UserNameHistoryParam { user_id }): Path<UserNameHistoryParam>,
    headers: HeaderMap,
) -> Result<impl IntoApiResponse> {
    check_logs_auth(&app, headers, None)?;
    app.check_opted_out(&user_id, None)?;

    let names = db::get_user_name_history(&app.db, &user_id).await?;

    Ok(Json(names))
}

fn cache_header(secs: u64) -> TypedHeader<CacheControl> {
    TypedHeader(
        CacheControl::new()
            .with_public()
            .with_max_age(Duration::from_secs(secs)),
    )
}

pub fn no_cache_header() -> TypedHeader<CacheControl> {
    TypedHeader(CacheControl::new().with_no_cache())
}

async fn resolve_user_params(params: &UserLogPathParams, app: &App) -> Result<(String, String)> {
    let channel_id = match params.channel_id_type {
        ChannelIdType::Name => app.get_user_id_by_name(&params.channel).await?,
        ChannelIdType::Id => params.channel.clone(),
    };
    let user_id = match params.user_id_type {
        UserIdType::Name => app.get_user_id_by_name(&params.user).await?,
        UserIdType::Id => params.user.clone(),
    };
    Ok((channel_id, user_id))
}
