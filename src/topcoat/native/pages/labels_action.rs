//! Typed page-label requests accepted by the durable account owner.

use super::super::{context, session};
use crate::{db::models::Page, error::LificError, realtime::RealtimeHub, services::pages};
use topcoat::{
    context::{Cx, app_context},
    runtime::{procedure, record},
};

#[record]
#[derive(Clone)]
pub(crate) struct Request {
    pub account_id: i64,
    pub page_id: i64,
    pub identifier: String,
    pub label: String,
    pub attach: bool,
}

#[record]
#[derive(Clone)]
pub(crate) struct Reply {
    pub status: Result<String, String>,
    pub account_id: i64,
    pub page_id: i64,
    pub canonical: Option<Snapshot>,
}

#[record]
#[derive(Clone)]
pub(crate) struct Snapshot {
    pub identifier: String,
    pub title: String,
    pub content: String,
    pub seq: i64,
    pub page_status: String,
    pub pinned: bool,
    pub labels: Vec<String>,
}

pub(crate) type RequestValue = <Request as topcoat::runtime::Surrogated>::Surrogate;
pub(crate) type ReplyValue = <Reply as topcoat::runtime::Surrogated>::Surrogate;

impl Snapshot {
    pub(crate) fn from_page(page: Page) -> Self {
        Self {
            identifier: page.identifier,
            title: page.title,
            content: page.content,
            seq: page.seq,
            page_status: page.status,
            pinned: page.pinned,
            labels: page.labels,
        }
    }
}

#[procedure("/__native_pages/labels")]
pub(crate) async fn update_labels(cx: &Cx, request: Request) -> topcoat::Result<Reply> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = match crate::api::require_user(&caller.identity) {
        Ok(user) => user,
        Err(error) => return session::read(cx, Err(error)),
    };
    if user.id != request.account_id {
        return Ok(failed(&request, "Your account changed. Reload this page."));
    }
    let db = context::db(cx);
    match pages::get(db, &caller.identity, request.page_id) {
        Ok(page) if page.identifier == request.identifier => {}
        Ok(_) | Err(LificError::NotFound(_)) => return Ok(failed(&request, "not found")),
        Err(error) => return Ok(failed(&request, error.client_message())),
    }
    let change = if request.attach {
        pages::PageLabelChange::Attach(&request.label)
    } else {
        pages::PageLabelChange::Remove(&request.label)
    };
    let result = caller
        .scope(async {
            pages::commit_label_change(
                db,
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                request.page_id,
                change,
            )
        })
        .await;
    Ok(match result {
        Ok(page) => Reply {
            status: Ok("saved".into()),
            account_id: user.id,
            page_id: page.id,
            canonical: Some(Snapshot::from_page(page)),
        },
        Err(LificError::NotFound(_)) => failed(&request, "not found"),
        Err(error) => failed(&request, error.client_message()),
    })
}

fn failed(request: &Request, message: &str) -> Reply {
    Reply {
        status: Err(message.to_owned()),
        account_id: request.account_id,
        page_id: request.page_id,
        canonical: None,
    }
}

#[cfg(test)]
mod production;
