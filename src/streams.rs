use crate::app::App;
use crate::db::schema::Stream;
use crate::ShutdownRx;
use std::borrow::Cow;
use std::collections::HashSet;
use std::mem;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::time::Instant;
use tracing::debug;
use twitch_api::helix::streams::GetStreamsRequest;
use twitch_api::helix::CursorRef;
use twitch_types::StreamId;

pub async fn run(
    app: App,
    writer_tx: Sender<Vec<Stream>>,
    mut shutdown_rx: ShutdownRx,
    request_interval: u64,
) {
    let mut last_loop = HashSet::<StreamId>::new();
    let mut current_loop = HashSet::<StreamId>::new();

    let timeout = tokio::time::sleep(Duration::ZERO);
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            _ = &mut timeout => {
                match query_streams(&app, &writer_tx, &mut last_loop, &mut current_loop, &mut shutdown_rx).await {
                    Err(e) => {
                        eprintln!("Error while querying streams: {:?}. Waiting a bit before retrying", e);
                        timeout.as_mut().reset(Instant::now() + Duration::from_secs(request_interval * 3));
                    },
                    Ok(_) => timeout.as_mut().reset(Instant::now() + Duration::from_secs(request_interval)),
                }
            }

            _ = shutdown_rx.changed() => {
                debug!("Shutting down streams task");
                break;
            }
        }
    }
}

async fn query_streams(
    app: &App,
    writer_tx: &Sender<Vec<Stream>>,
    last_loop: &mut HashSet<StreamId>,
    current_loop: &mut HashSet<StreamId>,
    shutdown_rx: &mut ShutdownRx,
) -> anyhow::Result<()> {
    debug!("Querying streams");
    let mut cursor: Option<Cow<'_, CursorRef>> = None;

    loop {
        let mut req = GetStreamsRequest::default().first(100);
        req.after = cursor;

        let resp = match app.helix_client.req_get(req, app.token.as_ref()).await {
            Ok(resp) => resp,
            Err(e) => {
                eprintln!("Error: {:?}", e);
                break;
            }
        };

        cursor = resp.pagination.map(Cow::Owned);
        let mut small_count = 0i8;
        let mut new_streams = Vec::with_capacity(50);
        new_streams.extend(resp.data.into_iter().filter_map(|s| {
            current_loop.insert(s.id.clone());
            if s.viewer_count < 2 {
                small_count += 1;
            }

            if last_loop.contains(&s.id) {
                return None;
            }

            Stream::try_from(s)
                .inspect_err(|e| eprintln!("Error while converting stream: {:?}", e))
                .ok()
        }));

        if !new_streams.is_empty() {
            writer_tx.send(new_streams).await?;
        }

        if cursor.is_none() || small_count > 50 || shutdown_rx.has_changed().unwrap_or(false) {
            break;
        }
    }

    mem::swap(last_loop, current_loop);
    current_loop.clear();

    Ok(())
}
