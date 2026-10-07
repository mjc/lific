//! Freshly authorized native graph relation procedures.
use topcoat::{
    context::{Cx, app_context},
    runtime::procedure,
};

use super::super::{context, session};
use crate::{error::LificError, realtime::RealtimeHub};

#[procedure("/__native_dependency_graph/link")]
pub(super) async fn link(
    cx: &Cx,
    account: i64,
    project_id: i64,
    source: String,
    target: String,
    relation_type: String,
) -> topcoat::Result<String> {
    let caller = session::read(cx, context::caller(cx))?;
    match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => {}
        Ok(_) => return Err(topcoat::router::error::forbidden().into()),
        Err(LificError::Forbidden(_)) => return Err(topcoat::router::error::forbidden().into()),
        Err(error) => return Err(error.into()),
    }
    let outcome = caller
        .scope(async {
            crate::services::dependency_graph::link(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                &source,
                &target,
                &relation_type,
                Some(project_id),
            )
        })
        .await;
    outcome.map(|_| "linked".to_owned()).or_else(route_error)
}

#[procedure("/__native_dependency_graph/unlink")]
pub(super) async fn unlink(
    cx: &Cx,
    account: i64,
    project_id: i64,
    source: String,
    target: String,
) -> topcoat::Result<String> {
    let caller = session::read(cx, context::caller(cx))?;
    match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => {}
        Ok(_) => return Err(topcoat::router::error::forbidden().into()),
        Err(LificError::Forbidden(_)) => return Err(topcoat::router::error::forbidden().into()),
        Err(error) => return Err(error.into()),
    }
    let outcome = caller
        .scope(async {
            crate::services::dependency_graph::unlink(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                &source,
                &target,
                Some(project_id),
            )
        })
        .await;
    outcome.map(|_| "unlinked".to_owned()).or_else(route_error)
}

#[procedure("/__native_dependency_graph/reverse")]
pub(super) async fn reverse(
    cx: &Cx,
    account: i64,
    project_id: i64,
    source: String,
    target: String,
) -> topcoat::Result<String> {
    let caller = session::read(cx, context::caller(cx))?;
    match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => {}
        Ok(_) => return Err(topcoat::router::error::forbidden().into()),
        Err(LificError::Forbidden(_)) => return Err(topcoat::router::error::forbidden().into()),
        Err(error) => return Err(error.into()),
    }
    let outcome = caller
        .scope(async {
            crate::services::dependency_graph::reverse(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                &source,
                &target,
                Some(project_id),
            )
        })
        .await;
    outcome.map(|_| "reversed".to_owned()).or_else(route_error)
}

fn route_error<T>(error: LificError) -> topcoat::Result<T> {
    match error {
        LificError::Forbidden(_) => Err(topcoat::router::error::forbidden().into()),
        LificError::NotFound(_) => Err(topcoat::router::error::not_found().into()),
        error => Err(error.into()),
    }
}
