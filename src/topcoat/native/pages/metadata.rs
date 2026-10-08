//! Shared lifecycle/metadata write policy for status and pin controls.

pub(super) struct FailureMessages {
    pub conflict: String,
    pub reauth: String,
    pub forbidden: String,
}

pub(super) fn failure_messages(conflict: &str) -> FailureMessages {
    FailureMessages {
        conflict: conflict.to_owned(),
        reauth: "Please sign in again.".to_owned(),
        forbidden: "You can no longer edit this page.".to_owned(),
    }
}
