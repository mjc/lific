//! Request-aware transport URLs for the pinned Topcoat runtime.

use std::sync::Arc;

use topcoat::{
    context::{Cx, try_app_context},
    router::request::{headers, remote_addr},
};

use crate::ratelimit::IpNetwork;

/// Only a configured direct proxy peer may supply the browser mount.
pub(crate) fn trusted_mount(cx: &Cx) -> Option<&str> {
    let peer = remote_addr(cx)?;
    let proxies = try_app_context::<Arc<[IpNetwork]>>(cx)?;
    if !proxies.iter().any(|proxy| proxy.contains(peer.ip())) {
        return None;
    }
    let prefix = headers(cx).get("x-forwarded-prefix")?.to_str().ok()?;
    super::super::session::forwarded_prefix(prefix)
}

/// `logical_path` is unmounted, even when its first segment equals the mount.
pub(crate) fn mounted_url(cx: &Cx, logical_path: &str) -> String {
    if logical_path.starts_with('/')
        && !logical_path.starts_with("//")
        && let Some(prefix) = trusted_mount(cx)
    {
        format!("{prefix}{logical_path}")
    } else {
        logical_path.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, sync::Arc};

    use topcoat::{context::CxTestBuilder, router::RemoteAddr};

    use super::*;
    use crate::ratelimit::IpNetwork;

    fn context(prefix: Option<&str>, peer: Option<&str>, proxies: Option<&[&str]>) -> Cx {
        let mut request = axum::http::Request::builder();
        if let Some(prefix) = prefix {
            request = request.header("x-forwarded-prefix", prefix);
        }
        let (mut parts, ()) = request.body(()).unwrap().into_parts();
        if let Some(peer) = peer {
            parts
                .extensions
                .insert(RemoteAddr(peer.parse::<SocketAddr>().unwrap()));
        }
        let mut context = CxTestBuilder::new().request_context(parts);
        if let Some(proxies) = proxies {
            let proxies: Arc<[IpNetwork]> = proxies
                .iter()
                .map(|proxy| IpNetwork::parse(proxy).unwrap())
                .collect();
            context = context.app_context(proxies);
        }
        context.build()
    }

    #[test]
    fn trusted_proxy_mount_is_validated_and_normalized_per_request() {
        for prefix in ["/app", "/ACC", "/nested/app/"] {
            let cx = context(Some(prefix), Some("127.0.0.1:5000"), Some(&["127.0.0.0/8"]));
            assert_eq!(trusted_mount(&cx), Some(prefix.trim_end_matches('/')));
        }
    }

    #[test]
    fn forwarded_prefix_requires_a_known_trusted_peer_and_proxy_configuration() {
        for (peer, proxies) in [
            (None, Some(&["127.0.0.0/8"][..])),
            (Some("192.0.2.1:5000"), Some(&["127.0.0.0/8"][..])),
            (Some("127.0.0.1:5000"), None),
        ] {
            let cx = context(Some("/app"), peer, proxies);
            assert_eq!(trusted_mount(&cx), None);
            assert_eq!(mounted_url(&cx, "/ACC/overview"), "/ACC/overview");
        }
        assert_eq!(trusted_mount(&CxTestBuilder::new().build()), None);
    }

    #[test]
    fn invalid_or_absent_forwarded_prefix_never_changes_urls() {
        for prefix in [
            None,
            Some("/"),
            Some("//host"),
            Some("/app/../other"),
            Some("https://host/app"),
            Some("/app?query"),
            Some("/app, /other"),
        ] {
            let cx = context(prefix, Some("127.0.0.1:5000"), Some(&["127.0.0.0/8"]));
            assert_eq!(trusted_mount(&cx), None);
            assert_eq!(
                mounted_url(&cx, "/__topcoat-runtime.js"),
                "/__topcoat-runtime.js"
            );
        }
    }

    #[test]
    fn logical_urls_are_mounted_once_without_prefix_collision_inference() {
        let cx = context(Some("/ACC"), Some("127.0.0.1:5000"), Some(&["127.0.0.0/8"]));
        assert_eq!(
            mounted_url(&cx, "/ACC/overview?view=a#details"),
            "/ACC/ACC/overview?view=a#details"
        );
        assert_eq!(mounted_url(&cx, "/"), "/ACC/");
        for external in [
            "//external.test/path",
            "https://external.test/path",
            "mailto:user@example.test",
            "#details",
            "?view=a",
            "relative/path",
        ] {
            assert_eq!(mounted_url(&cx, external), external);
        }
    }
}
