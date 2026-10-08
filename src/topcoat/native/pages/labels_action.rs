//! Typed page-label requests accepted by the durable account owner.

use topcoat::runtime::record;

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

#[cfg(test)]
mod production;
