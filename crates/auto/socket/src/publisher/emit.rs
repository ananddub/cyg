use super::PublishError;
use crate::state::{SOCKET_IO, StreamHistory, latest_channels, stream_channels, stream_history};
use std::collections::VecDeque;

pub(super) async fn emit_room(
    namespace: &'static str,
    room: String,
    message: serde_json::Value,
) -> Result<(), PublishError> {
    let io = SOCKET_IO.get().ok_or(PublishError::NotRegistered)?;
    let endpoint = message
        .get("endpoint")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let args = message.get("args").cloned().unwrap_or_default();
    if let Some(socket_namespace) = io.of(namespace) {
        tracing::info!(namespace, room = %room, endpoint, args = ?args, "sending live update to socket room");
        let result = socket_namespace
            .to(room)
            .emit("live:update", &message)
            .await;
        match result {
            Ok(()) => {
                tracing::info!(namespace, endpoint, "live update sent to socket room");
            }
            Err(error) => {
                tracing::warn!(namespace, endpoint, error = %error, "failed to send live update to socket room");
                return Err(error.into());
            }
        }
    } else {
        tracing::warn!(
            namespace,
            endpoint,
            "live socket namespace is not registered"
        );
    }
    Ok(())
}

pub(super) fn publish_latest(
    namespace: &'static str,
    room: String,
    message: serde_json::Value,
) -> Result<(), PublishError> {
    SOCKET_IO.get().ok_or(PublishError::NotRegistered)?;
    let key = format!("{namespace}:{room}");
    let channels = latest_channels();
    if let Some(sender) = channels.read_sync(&key, |_, s| s.clone()) {
        sender.send_replace(message);
        return Ok(());
    }
    let (sender, mut receiver) = tokio::sync::watch::channel(message);
    match channels.entry_sync(key) {
        scc::hash_map::Entry::Occupied(occ) => {
            occ.get().send_replace(receiver.borrow().clone());
        }
        scc::hash_map::Entry::Vacant(vac) => {
            vac.insert_entry(sender);
            tokio::spawn(async move {
                loop {
                    let value = receiver.borrow_and_update().clone();
                    let _ = emit_room(namespace, room.clone(), value).await;
                    if receiver.changed().await.is_err() {
                        break;
                    }
                }
            });
        }
    }
    Ok(())
}

pub(super) async fn publish_stream(
    namespace: &'static str,
    room: String,
    message: serde_json::Value,
    capacity: usize,
    replay: usize,
) -> Result<(), PublishError> {
    SOCKET_IO.get().ok_or(PublishError::NotRegistered)?;
    let key = format!("{namespace}:{room}");
    if replay > 0 {
        let histories = stream_history();
        match histories.entry_sync(key.clone()) {
            scc::hash_map::Entry::Occupied(mut occ) => {
                let history = occ.get_mut();
                history.limit = replay;
                while history.events.len() >= history.limit {
                    history.events.pop_front();
                }
                history.events.push_back(message.clone());
            }
            scc::hash_map::Entry::Vacant(vac) => {
                let mut events = VecDeque::with_capacity(replay);
                events.push_back(message.clone());
                vac.insert_entry(StreamHistory {
                    limit: replay,
                    events,
                });
            }
        }
    }
    let sender = {
        let channels = stream_channels();
        if let Some(sender) = channels.read_sync(&key, |_, s| s.clone()) {
            sender
        } else {
            let (sender, mut receiver) = tokio::sync::mpsc::channel(capacity.max(1));
            match channels.entry_sync(key) {
                scc::hash_map::Entry::Occupied(occ) => occ.get().clone(),
                scc::hash_map::Entry::Vacant(vac) => {
                    vac.insert_entry(sender.clone());
                    tokio::spawn(async move {
                        while let Some(value) = receiver.recv().await {
                            let _ = emit_room(namespace, room.clone(), value).await;
                        }
                    });
                    sender
                }
            }
        }
    };
    sender
        .send(message)
        .await
        .map_err(|_| PublishError::NotRegistered)
}
