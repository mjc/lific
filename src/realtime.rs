use axum::extract::ws::{Message, WebSocket};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tracing::warn;

const EVENT_BUFFER: usize = 256;

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
        let _ = self.tx.send(event);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RealtimeEvent {
    #[serde(rename = "resync.required")]
    ResyncRequired,
    #[serde(rename = "project.created")]
    ProjectCreated {
        project_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "project.updated")]
    ProjectUpdated {
        project_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "project.deleted")]
    ProjectDeleted {
        project_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "projects.reordered")]
    ProjectsReordered,
    #[serde(rename = "issue.created")]
    IssueCreated {
        project_id: i64,
        issue_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "issue.updated")]
    IssueUpdated {
        project_id: i64,
        issue_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "issue.deleted")]
    IssueDeleted {
        project_id: i64,
        issue_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "issue.linked")]
    IssueLinked { project_id: i64, issue_id: i64 },
    #[serde(rename = "issue.unlinked")]
    IssueUnlinked { project_id: i64, issue_id: i64 },
    #[serde(rename = "comment.created")]
    CommentCreated {
        comment_id: i64,
        issue_id: Option<i64>,
        page_id: Option<i64>,
    },
    #[serde(rename = "comment.updated")]
    CommentUpdated {
        comment_id: i64,
        issue_id: Option<i64>,
        page_id: Option<i64>,
    },
    #[serde(rename = "comment.deleted")]
    CommentDeleted {
        comment_id: i64,
        issue_id: Option<i64>,
        page_id: Option<i64>,
    },
    #[serde(rename = "page.created")]
    PageCreated {
        project_id: Option<i64>,
        page_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "page.updated")]
    PageUpdated {
        project_id: Option<i64>,
        page_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "page.deleted")]
    PageDeleted {
        project_id: Option<i64>,
        page_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "plan.created")]
    PlanCreated {
        project_id: i64,
        plan_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "plan.updated")]
    PlanUpdated {
        project_id: i64,
        plan_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "plan.deleted")]
    PlanDeleted {
        project_id: i64,
        plan_id: i64,
        identifier: Option<String>,
    },
    #[serde(rename = "plan.step.updated")]
    PlanStepUpdated {
        project_id: i64,
        plan_id: i64,
        step_id: i64,
    },
    #[serde(rename = "module.created")]
    ModuleCreated {
        project_id: i64,
        module_id: i64,
        name: Option<String>,
    },
    #[serde(rename = "module.updated")]
    ModuleUpdated {
        project_id: i64,
        module_id: i64,
        name: Option<String>,
    },
    #[serde(rename = "module.deleted")]
    ModuleDeleted {
        project_id: i64,
        module_id: i64,
        name: Option<String>,
    },
    #[serde(rename = "label.created")]
    LabelCreated {
        project_id: i64,
        label_id: i64,
        name: Option<String>,
    },
    #[serde(rename = "label.deleted")]
    LabelDeleted {
        project_id: i64,
        label_id: i64,
        name: Option<String>,
    },
    #[serde(rename = "folder.created")]
    FolderCreated {
        project_id: Option<i64>,
        folder_id: i64,
        name: Option<String>,
    },
    #[serde(rename = "folder.deleted")]
    FolderDeleted {
        project_id: Option<i64>,
        folder_id: i64,
        name: Option<String>,
    },
}

pub async fn serve_socket(mut socket: WebSocket, hub: RealtimeHub) {
    let mut rx = hub.subscribe();

    loop {
        tokio::select! {
            maybe_event = rx.recv() => {
                match maybe_event {
                    Ok(event) => {
                        if socket.send(Message::Text(serde_json::to_string(&event).expect("realtime event serializes").into())).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(dropped)) => {
                        warn!(dropped, "realtime websocket lagged; asking client to resync");
                        let event = RealtimeEvent::ResyncRequired;
                        if socket.send(Message::Text(serde_json::to_string(&event).expect("resync event serializes").into())).await.is_err() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_serializes_with_dotted_type() {
        let event = RealtimeEvent::IssueUpdated {
            project_id: 7,
            issue_id: 42,
            identifier: Some("LIF-42".into()),
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
            identifier: Some("LIF".into()),
        });
        hub.send(RealtimeEvent::ProjectUpdated {
            project_id: 2,
            identifier: Some("LIF".into()),
        });

        assert!(matches!(rx.recv().await, Err(RecvError::Lagged(1))));
        assert_eq!(
            rx.recv().await.unwrap(),
            RealtimeEvent::ProjectUpdated {
                project_id: 2,
                identifier: Some("LIF".into()),
            }
        );
    }
}
