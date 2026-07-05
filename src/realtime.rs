use axum::extract::ws::{Message, WebSocket};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{self, Duration};
use tracing::{trace, warn};

const EVENT_BUFFER: usize = 256;
const SESSION_REVALIDATE_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone)]
pub struct RealtimeHub {
    tx: broadcast::Sender<RealtimeEvent>,
}

impl RealtimeHub {
    pub fn new() -> Self {
        Self::with_capacity(EVENT_BUFFER)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RealtimeEvent> {
        self.tx.subscribe()
    }

    pub fn send(&self, event: RealtimeEvent) {
        if self.tx.send(event).is_err() {
            trace!("dropped realtime event because no receivers are subscribed");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RealtimeEvent {
    #[serde(rename = "resync.required")]
    ResyncRequired,
    #[serde(rename = "project.created")]
    ProjectCreated { project_id: i64, identifier: String },
    #[serde(rename = "project.updated")]
    ProjectUpdated { project_id: i64, identifier: String },
    #[serde(rename = "project.deleted")]
    ProjectDeleted { project_id: i64, identifier: String },
    #[serde(rename = "projects.reordered")]
    ProjectsReordered,
    #[serde(rename = "issue.created")]
    IssueCreated {
        project_id: i64,
        issue_id: i64,
        identifier: String,
    },
    #[serde(rename = "issue.updated")]
    IssueUpdated {
        project_id: i64,
        issue_id: i64,
        identifier: String,
    },
    #[serde(rename = "issue.deleted")]
    IssueDeleted {
        project_id: i64,
        issue_id: i64,
        identifier: String,
    },
    #[serde(rename = "issue.linked")]
    IssueLinked { project_id: i64, issue_id: i64 },
    #[serde(rename = "issue.unlinked")]
    IssueUnlinked { project_id: i64, issue_id: i64 },
}

pub async fn serve_socket(
    mut socket: WebSocket,
    hub: RealtimeHub,
    db: crate::db::DbPool,
    session_token: String,
) {
    let mut rx = hub.subscribe();
    let mut revalidate = time::interval(SESSION_REVALIDATE_INTERVAL);
    revalidate.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = revalidate.tick() => {
                if !session_is_valid(&db, &session_token) {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            }
            maybe_event = rx.recv() => {
                match maybe_event {
                    Ok(event) => {
                        if send_event(&mut socket, &event).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(dropped)) => {
                        warn!(dropped, "realtime websocket lagged; asking client to resync");
                        let event = RealtimeEvent::ResyncRequired;
                        if send_event(&mut socket, &event).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            maybe_msg = socket.recv() => {
                match maybe_msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
}

async fn send_event(socket: &mut WebSocket, event: &RealtimeEvent) -> Result<(), axum::Error> {
    match serde_json::to_string(event) {
        Ok(json) => socket.send(Message::Text(json.into())).await,
        Err(_) => {
            warn!("failed to serialize realtime event");
            socket.send(Message::Close(None)).await
        }
    }
}

fn session_is_valid(db: &crate::db::DbPool, token: &str) -> bool {
    db.read()
        .ok()
        .and_then(|conn| crate::db::queries::users::validate_session(&conn, token).ok())
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_serializes_with_dotted_type() {
        let event = RealtimeEvent::IssueUpdated {
            project_id: 7,
            issue_id: 42,
            identifier: "LIF-42".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "issue.updated");
        assert_eq!(json["project_id"], 7);
        assert_eq!(json["issue_id"], 42);
        assert_eq!(json["identifier"], "LIF-42");
    }

    #[tokio::test]
    async fn lagged_receiver_requests_resync() {
        let hub = RealtimeHub::with_capacity(1);
        let mut rx = hub.subscribe();

        hub.send(RealtimeEvent::ProjectUpdated {
            project_id: 1,
            identifier: "LIF".into(),
        });
        hub.send(RealtimeEvent::ProjectUpdated {
            project_id: 2,
            identifier: "LIF".into(),
        });

        assert!(matches!(rx.recv().await, Err(RecvError::Lagged(1))));
        assert_eq!(
            rx.recv().await.unwrap(),
            RealtimeEvent::ProjectUpdated {
                project_id: 2,
                identifier: "LIF".into(),
            }
        );
    }
}
