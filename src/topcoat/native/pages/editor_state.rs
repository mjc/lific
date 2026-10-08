//! Canonical PageDetail fields shared by independently rendered controls.

use crate::db::models::Page;
use topcoat::{
    context::Cx,
    runtime::{Signal, signal},
};

#[derive(Clone)]
pub(super) struct EditorState {
    pub(super) title: Signal<String>,
    pub(super) body: Signal<String>,
    pub(super) title_draft: Signal<String>,
    pub(super) body_draft: Signal<String>,
    pub(super) seq: Signal<i64>,
    pub(super) status: Signal<String>,
    pub(super) pinned: Signal<bool>,
    pub(super) labels: Signal<Vec<String>>,
    pub(super) busy: Signal<bool>,
}

impl EditorState {
    pub(super) fn new(cx: &Cx, page: &Page) -> Self {
        Self {
            title: signal(cx, || page.title.clone()),
            body: signal(cx, || page.content.clone()),
            title_draft: signal(cx, || page.title.clone()),
            body_draft: signal(cx, || page.content.clone()),
            seq: signal(cx, || page.seq),
            status: signal(cx, || page.status.clone()),
            pinned: signal(cx, || page.pinned),
            labels: signal(cx, || page.labels.clone()),
            busy: signal(cx, || false),
        }
    }
}
