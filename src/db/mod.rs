use std::collections::HashMap;
use axum::extract::State;

use chrono::{Datelike, DateTime, Duration, Utc};
use clickhouse::{Client, query::RowCursor};
use rand::{seq::IteratorRandom, thread_rng};
use tracing::debug;

pub use migrations::run as setup_db;
use writer::FlushBuffer;
use schema::StructuredMessage;

use crate::{
    error::Error,
    logs::{
        schema::LogRangeParams,
        stream::{FlushBufferResponse, LogsStream},
    },
    Result,
    web::schema::{AvailableLogDate, LogsParams, UserHasLogs},
};
use crate::app::App;
use crate::web::schema::{UserLogins, UserParam};

mod migrations;
pub mod schema;
pub mod writer;

const CHANNEL_MULTI_QUERY_SIZE_DAYS: i64 = 14;

pub async fn read_channel(
    db: &Client,
    channel_id: &str,
    params: LogRangeParams,
    flush_buffer: &FlushBuffer,
) -> Result<LogsStream> {
    let suffix = if params.logs_params.reverse {
        "DESC"
    } else {
        "ASC"
    };

    let mut query = format!("SELECT ?fields FROM message_structured WHERE channel_id = ? AND timestamp >= ? AND timestamp < ? ORDER BY timestamp {suffix}");

    let flush_params = FlushBufferResponse {
        buffer: Some(flush_buffer.clone()),
        channel_id: channel_id.to_owned(),
        user_id: None,
        params,
    };

    let interval = Duration::days(CHANNEL_MULTI_QUERY_SIZE_DAYS);
    if params.to - params.from > interval {
        let count = db
            .query("SELECT count() FROM (SELECT timestamp FROM message_structured WHERE channel_id = ? AND timestamp >= ? AND timestamp < ? LIMIT 1)")
            .bind(channel_id)
            .bind(params.from.timestamp_millis() as f64 / 1000.0)
            .bind(params.to.timestamp_millis() as f64 / 1000.0)
            .fetch_one::<i32>().await?;
        if count == 0 {
            return Err(Error::NotFound);
        }

        let mut streams = Vec::with_capacity(1);

        let mut current_from = params.from;
        let mut current_to = current_from + interval;

        loop {
            let cursor = next_cursor(db, &query, channel_id, current_from, current_to)?;
            streams.push(cursor);

            current_from += interval;
            current_to += interval;

            if current_to > params.to {
                let cursor = next_cursor(db, &query, channel_id, current_from, params.to)?;
                streams.push(cursor);
                break;
            }
        }

        if params.logs_params.reverse {
            streams.reverse();
        }

        debug!("Using {} queries for multi-query stream", streams.len());

        LogsStream::new_multi_query(streams, flush_params)
    } else {
        apply_limit_offset(
            &mut query,
            params.logs_params.limit,
            params.logs_params.offset,
        );

        let cursor = db
            .query(&query)
            .bind(channel_id)
            .bind(params.from.timestamp_millis() as f64 / 1000.0)
            .bind(params.to.timestamp_millis() as f64 / 1000.0)
            .fetch()?;
        LogsStream::new_cursor(cursor, flush_params).await
    }
}

fn next_cursor(
    db: &Client,
    query: &str,
    channel_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<RowCursor<StructuredMessage<'static>>> {
    let cursor = db
        .query(query)
        .bind(channel_id)
        .bind(from.timestamp_millis() as f64 / 1000.0)
        .bind(to.timestamp_millis() as f64 / 1000.0)
        .fetch()?;
    Ok(cursor)
}

pub async fn read_user(
    db: &Client,
    channel_id: &str,
    user_id: &str,
    params: LogRangeParams,
    flush_buffer: &FlushBuffer,
) -> Result<LogsStream> {
    let suffix = if params.logs_params.reverse {
        "DESC"
    } else {
        "ASC"
    };
    let mut query = format!("SELECT * FROM message_structured WHERE channel_id = ? AND user_id = ? AND timestamp >= ? AND timestamp < ? ORDER BY timestamp {suffix}");
    apply_limit_offset(
        &mut query,
        params.logs_params.limit,
        params.logs_params.offset,
    );

    let flush_params = FlushBufferResponse {
        buffer: Some(flush_buffer.clone()),
        channel_id: channel_id.to_owned(),
        user_id: Some(user_id.to_owned()),
        params,
    };

    let cursor = db
        .query(&query)
        .bind(channel_id)
        .bind(user_id)
        .bind(params.from.timestamp_millis() as f64 / 1000.0)
        .bind(params.to.timestamp_millis() as f64 / 1000.0)
        .fetch()?;
    LogsStream::new_cursor(cursor, flush_params).await
}

pub async fn read_available_channel_logs(
    db: &Client,
    channel_id: &str,
) -> Result<Vec<AvailableLogDate>> {
    let timestamps: Vec<i32> = db
        .query(
            "SELECT toDateTime(toStartOfDay(timestamp)) AS date FROM message_structured WHERE channel_id = ? GROUP BY date ORDER BY date DESC",
        )
        .bind(channel_id)
        .fetch_all().await?;

    let dates = timestamps
        .into_iter()
        .map(|timestamp| {
            let naive = DateTime::from_timestamp(timestamp.into(), 0).expect("Invalid DateTime");

            AvailableLogDate {
                year: naive.year().to_string(),
                month: naive.month().to_string(),
                day: Some(naive.day().to_string()),
            }
        })
        .collect();

    Ok(dates)
}

pub async fn read_available_user_logs(
    db: &Client,
    channel_id: &str,
    user_id: &str,
) -> Result<Vec<AvailableLogDate>> {
    let timestamps: Vec<i32> = db
        .query("SELECT toDateTime(toStartOfMonth(timestamp)) AS date FROM message_structured WHERE channel_id = ? AND user_id = ? GROUP BY date ORDER BY date DESC")
        .bind(channel_id)
        .bind(user_id)
        .fetch_all().await?;

    let dates = timestamps
        .into_iter()
        .map(|timestamp| {
            let naive = DateTime::from_timestamp(timestamp.into(), 0).expect("Invalid DateTime");

            AvailableLogDate {
                year: naive.year().to_string(),
                month: naive.month().to_string(),
                day: None,
            }
        })
        .collect();

    Ok(dates)
}

pub async fn read_random_user_line(
    db: &Client,
    channel_id: &str,
    user_id: &str,
) -> Result<StructuredMessage<'static>> {
    let total_count = db
        .query("SELECT count(*) FROM message_structured WHERE channel_id = ? AND user_id = ? ")
        .bind(channel_id)
        .bind(user_id)
        .fetch_one::<u64>()
        .await?;

    if total_count == 0 {
        return Err(Error::NotFound);
    }

    let offset = {
        let mut rng = thread_rng();
        (0..total_count).choose(&mut rng).ok_or(Error::NotFound)
    }?;

    let msg = db
        .query(
            "WITH
            (SELECT timestamp FROM message_structured WHERE channel_id = ? AND user_id = ? LIMIT 1 OFFSET ?)
            AS random_timestamp
            SELECT * FROM message_structured WHERE channel_id = ? AND user_id = ? AND timestamp = random_timestamp",
        )
        .bind(channel_id)
        .bind(user_id)
        .bind(offset)
        .bind(channel_id)
        .bind(user_id)
        .fetch_optional::<StructuredMessage>()
        .await?
        .ok_or(Error::NotFound)?;

    Ok(msg)
}

pub async fn read_random_channel_line(
    db: &Client,
    channel_id: &str,
) -> Result<StructuredMessage<'static>> {
    let total_count = db
        .query("SELECT count(*) FROM message_structured WHERE channel_id = ? ")
        .bind(channel_id)
        .fetch_one::<u64>()
        .await?;

    if total_count == 0 {
        return Err(Error::NotFound);
    }

    let offset = {
        let mut rng = thread_rng();
        (0..total_count).choose(&mut rng).ok_or(Error::NotFound)
    }?;

    let msg = db
        .query(
            "WITH
            (SELECT timestamp FROM message_structured WHERE channel_id = ? LIMIT 1 OFFSET ?)
            AS random_timestamp
            SELECT * FROM message_structured WHERE channel_id = ? AND timestamp = random_timestamp",
        )
        .bind(channel_id)
        .bind(offset)
        .bind(channel_id)
        .fetch_optional::<StructuredMessage>()
        .await?
        .ok_or(Error::NotFound)?;

    Ok(msg)
}

pub async fn check_users_exist(db: &Client, channel_id: &str, user_ids: &[String]) -> Result<Vec<UserHasLogs>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = user_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let query = format!("SELECT user_id FROM message_structured WHERE channel_id = ? AND user_id IN ({}) GROUP BY user_id", placeholders);

    let mut query_builder = db.query(&query).bind(channel_id);
    for user_id in user_ids {
        query_builder = query_builder.bind(user_id);
    }

    let mut user_has_logs = user_ids
        .iter()
        .map(|id| (id.clone(), UserHasLogs {
            user: id.clone(),
            has_logs: false,
        })).collect::<HashMap<String, UserHasLogs>>();

    query_builder.fetch_all::<String>().await?.into_iter().for_each(|user_id| {
        if let Some(user) = user_has_logs.get_mut(&user_id) {
            user.has_logs = true;
        }
    });

    Ok(user_has_logs.into_values().collect())
}

pub async fn search_user_logins(app: &State<App>, param: &UserParam) -> Result<UserLogins> {
    let db = &app.db;
    let id = match param {
        UserParam::UserId(id) => id.to_string(),
        UserParam::User(login) => {
            // try to fetch the user ID from the database
            let db_result = db.query("SELECT user_id FROM message_structured WHERE user_login = ? AND user_id != '' LIMIT 1")
                .bind(login)
                .fetch_optional::<String>()
                .await?;

            // user id isnt stored in db, so try fetching it via helix
            if let Some(user_id) = db_result {
                user_id
            } else {
                app.get_user_id_by_name(login).await?
            }
        }
    };

    if id.is_empty() {
        return Err(Error::NotFound);
    }

    let query = db.query("SELECT user_login FROM message_structured WHERE user_id = ? GROUP BY user_login").bind(id.clone());

    let logins = query.fetch_all::<String>().await?;
    Ok(UserLogins { id, logins })
}

pub async fn search_user_logs(
    db: &Client,
    channel_id: &str,
    user_id: &str,
    search: &str,
    params: LogsParams,
) -> Result<LogsStream> {
    let suffix = if params.reverse { "DESC" } else { "ASC" };

    let mut query = format!("SELECT * FROM message_structured WHERE channel_id = ? AND user_id = ? AND positionCaseInsensitive(text, ?) != 0 ORDER BY timestamp {suffix}");
    apply_limit_offset(&mut query, params.limit, params.offset);

    let cursor = db
        .query(&query)
        .bind(channel_id)
        .bind(user_id)
        .bind(search)
        .fetch()?;

    let flush_params = FlushBufferResponse {
        buffer: None,
        channel_id: String::new(),
        user_id: None,
        params: LogRangeParams {
            from: DateTime::UNIX_EPOCH,
            to: DateTime::UNIX_EPOCH,
            logs_params: params,
        },
    };
    LogsStream::new_cursor(cursor, flush_params).await
}

fn apply_limit_offset(query: &mut String, limit: Option<u64>, offset: Option<u64>) {
    if let Some(limit) = limit {
        *query = format!("{query} LIMIT {limit}");
    }
    if let Some(offset) = offset {
        *query = format!("{query} OFFSET {offset}");
    }
}
