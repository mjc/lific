//! Rust-rendered links for safe internal page navigation.

use topcoat::{
    context::Cx,
    runtime::{Event, Js, PrefetchMode, expr, link_attrs, prefetch_mode},
    view::Attributes,
};

use super::transport::mounted_url;
use super::workspace::native_navigation_authorized;

/// Adds Topcoat navigation and prefetch attributes to a logical internal URL.
///
/// Entry tokens are consumed when a route is prepared, so prefetching those
/// destinations could consume state before the user follows the link.
pub(crate) fn attrs(cx: &Cx, logical_path: &str) -> Attributes {
    let prefetch = if has_entry_token(logical_path) {
        PrefetchMode::Never
    } else {
        prefetch_mode(cx)
    };
    link_attrs(cx, mounted_url(cx, logical_path), prefetch)
}

fn has_entry_token(path: &str) -> bool {
    path.split('#')
        .next()
        .and_then(|path| path.split_once('?').map(|(_, query)| query))
        .is_some_and(|query| {
            query.split('&').any(|part| {
                matches!(
                    part.split('=').next(),
                    Some("notice" | "management_resume" | "resume")
                )
            })
        })
}

pub(crate) fn authority_mount(cx: &Cx) -> Attributes {
    let key = format!("{}#navigation-authority", super::home_shell::handler_url());
    super::handler_asset::event(cx, &key, Js::source("[]"), "mount")
}

pub(crate) fn authority_handler_factory() -> Js {
    expr!(|_mount: Event| {
        let _before_commit = |_event: Event| {
            let destination = raw!(
                "cx.hydrate(${_event}.detail.url.pathname + ${_event}.detail.url.search)",
                String::new()
            );
            let _full_destination = raw!(
                "cx.hydrate(${_event}.detail.url.href)",
                String::new()
            );
            let _check = async || {
                let cancelled = raw!(
                    "cx.hydrate(${_event}.detail.signal.aborted || cx.abortSignal.aborted)",
                    false
                );
                if !cancelled {
                    let incoming_account = raw!(
                        "cx.hydrate({t:'i64',bits:64,v:${_event}.detail.nextDocument.querySelector('.native-home-shell')?.dataset.accountId || '0'})",
                        0_i64
                    );
                    let incoming_admin = raw!(
                        "cx.hydrate(${_event}.detail.nextDocument.querySelector('.native-home-shell')?.dataset.accountAdmin === 'true')",
                        false
                    );
                    let incoming_authority = raw!(
                        "cx.hydrate(${_event}.detail.nextDocument.querySelector('[data-native-project-authority]')?.dataset.nativeProjectAuthority || '')",
                        String::new()
                    );
                    let verdict = native_navigation_authorized(
                        destination.clone(),
                        incoming_account,
                        incoming_admin,
                        incoming_authority,
                    )
                    .await;
                    let cancelled = raw!(
                        "cx.hydrate(${_event}.detail.signal.aborted || cx.abortSignal.aborted)",
                        false
                    );
                    if !cancelled {
                        if verdict != "allow" {
                            raw!(
                                "location.assign(${_full_destination}.toString()); throw new Error('navigation destination authority changed')",
                                ()
                            );
                        }
                    }
                }
            };
            raw!(
                "${_event}.detail.waitUntil(Promise.resolve().then(() => ${_check}()));",
                ()
            );
        };
        raw!(
            "document.addEventListener('topcoat:before-navigation-commit', ${_before_commit}, {signal:cx.abortSignal});",
            ()
        );
    })
    .into_evaluated_and_js()
    .1
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, sync::Arc};

    use topcoat::{
        context::{Cx, CxTestBuilder},
        router::RemoteAddr,
        view::{ViewExt, view},
    };

    use super::*;
    use crate::ratelimit::IpNetwork;

    fn context(prefix: Option<&str>) -> Cx {
        let request = if let Some(prefix) = prefix {
            axum::http::Request::builder()
                .header("x-forwarded-prefix", prefix)
                .body(())
                .unwrap()
        } else {
            axum::http::Request::new(())
        };
        let (mut parts, ()) = request.into_parts();
        if prefix.is_some() {
            parts
                .extensions
                .insert(RemoteAddr("127.0.0.1:5000".parse::<SocketAddr>().unwrap()));
        }
        let mut builder = CxTestBuilder::new().request_context(parts);
        if prefix.is_some() {
            let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
            builder = builder.app_context(proxies);
        }
        builder.build()
    }

    #[tokio::test]
    async fn internal_links_mount_once_and_use_the_context_prefetch_mode() {
        let cx = context(Some("/ACC"));
        let attrs = attrs(&cx, "/ACC/issues/ACC-1");
        let html = view! { cx => <a (attrs)>"Issue"</a> }
            .single()
            .await
            .unwrap()
            .render(&cx);

        assert!(html.contains(r#"href="/ACC/ACC/issues/ACC-1""#), "{html}");
        assert!(html.contains(r#"data-topcoat-link="intent""#), "{html}");
    }

    #[tokio::test]
    async fn entry_token_links_disable_prefetch_and_plain_external_downloads_stay_plain() {
        let cx = Cx::default();
        for path in [
            "/ACC/overview?notice=created",
            "/ACC/overview?management_resume=token",
            "/projects/new?resume=token",
        ] {
            let attrs = attrs(&cx, path);
            let html = view! { cx => <a (attrs)>"Continue"</a> }
                .single()
                .await
                .unwrap()
                .render(&cx);
            assert!(
                html.contains(r#"data-topcoat-link="never""#),
                "{path}: {html}"
            );
        }

        let html = view! {
            cx =>
            <a href="https://example.test/guide" target="_blank" rel="noopener">
                "External"
            </a>
            <a href="/export.csv" download="export.csv">"Download"</a>
        }
        .single()
        .await
        .unwrap()
        .render(&cx);
        assert!(!html.contains("data-topcoat-link"), "{html}");
        assert!(
            html.contains(r#"href="https://example.test/guide""#),
            "{html}"
        );
        assert!(html.contains(r#"download="export.csv""#), "{html}");
    }
}
