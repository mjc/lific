//! Project icons follow the original master's approved icon and emoji data.

use std::{
    collections::{BTreeMap, HashSet},
    sync::LazyLock,
};

use topcoat::{
    context::Cx,
    view::{Attributes, BoxView, ViewExt, view},
};

use super::transport::mounted_url;

type IconNodes = Vec<(String, BTreeMap<String, String>)>;

// These are checked-in package data, never markup supplied by a project.
// See assets/icons.LICENSE.txt for the original versions and provenance.
static ICONS: LazyLock<BTreeMap<String, IconNodes>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("assets/project-icons.json"))
        .expect("the frozen original Lucide icon data is valid")
});
static EMOJI: LazyLock<HashSet<String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("assets/emoji.json"))
        .expect("the frozen original emoji allowlist is valid")
});

pub(crate) fn project_icon<'a>(cx: &'a Cx, value: Option<&str>, size: u32) -> BoxView<'a> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return view! { cx => "" }.boxed();
    };
    if value == "lific:logo" {
        let src = mounted_url(cx, "/logo.webp");
        let style = format!(
            "width: {size}px; height: {size}px; object-fit: contain; display: inline-block;"
        );
        return view! { cx => <img src=(src) alt="Lific" style=(style)> }.boxed();
    }
    if EMOJI.contains(value) {
        let value = value.to_owned();
        let style = format!(
            "display: inline-flex; flex-shrink: 0; align-items: center; justify-content: center; overflow: hidden; width: {size}px; height: {size}px; font-size: {size}px; line-height: 1;"
        );
        return view! { cx => <span style=(style)>(value)</span> }.boxed();
    }
    let nodes = value
        .strip_prefix("lucide:")
        .and_then(|name| ICONS.get(name))
        .unwrap_or_else(|| {
            ICONS
                .get("Folder")
                .expect("original Lucide includes Folder")
        });
    let nodes: Vec<_> = nodes
        .iter()
        .map(|(tag, values)| {
            let mut attributes = Attributes::with_capacity(values.len());
            for (key, value) in values {
                attributes.insert(cx, key.as_str(), value.as_str());
            }
            (tag.as_str(), attributes)
        })
        .collect();
    view! { cx =>
        <svg width=(size) height=(size) viewBox="0 0 24 24" fill="none"
            stroke="currentColor" stroke-width="2" stroke-linecap="round"
            stroke-linejoin="round" aria-hidden="true" style="flex-shrink: 0;">
            for (tag, attributes) in nodes {
                <(tag) (attributes)/>
            }
        </svg>
    }
    .boxed()
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, sync::Arc};

    use topcoat::{context::CxTestBuilder, router::RemoteAddr, view::ViewExt};

    use super::*;
    use crate::ratelimit::IpNetwork;

    async fn render(cx: &Cx, value: Option<&str>, size: u32) -> String {
        project_icon(cx, value, size)
            .single()
            .await
            .unwrap()
            .render(cx)
    }

    #[tokio::test]
    async fn empty_project_icon_renders_nothing() {
        let cx = Cx::default();
        for value in [None, Some("")] {
            assert_eq!(render(&cx, value, 15).await, "");
        }
    }

    #[tokio::test]
    async fn approved_emoji_keeps_original_size_and_text() {
        let html = render(&Cx::default(), Some("🦎"), 15).await;
        assert!(html.starts_with("<span "));
        assert!(html.contains("width: 15px; height: 15px; font-size: 15px; line-height: 1;"));
        assert!(html.contains(">🦎</span>"));
        assert!(!html.contains("<svg"));
    }

    #[tokio::test]
    async fn approved_lucide_uses_original_svg_geometry_at_requested_size() {
        let html = render(&Cx::default(), Some("lucide:Circle"), 19).await;
        for attribute in [
            "width=\"19\"",
            "height=\"19\"",
            "viewBox=\"0 0 24 24\"",
            "fill=\"none\"",
            "stroke=\"currentColor\"",
            "stroke-width=\"2\"",
            "stroke-linecap=\"round\"",
            "stroke-linejoin=\"round\"",
            "cx=\"12\"",
            "cy=\"12\"",
            "r=\"10\"",
        ] {
            assert!(html.contains(attribute), "missing {attribute}: {html}");
        }
        assert!(html.contains("<circle "));
        assert!(!html.contains("lucide:Circle"));
    }

    #[tokio::test]
    async fn invalid_nonempty_values_render_folder_without_user_markup() {
        let cx = Cx::default();
        let folder = render(&cx, Some("lucide:Folder"), 15).await;
        for value in [
            "lucide:constructor",
            "lucide:Unknown",
            "plain text",
            "<script>alert(1)</script>",
        ] {
            let html = render(&cx, Some(value), 15).await;
            assert_eq!(html, folder);
            assert!(!html.contains("<script"));
            assert!(!html.contains("alert(1)"));
        }
    }

    #[tokio::test]
    async fn logo_uses_original_asset_with_trusted_mount_and_original_alt() {
        let (mut parts, ()) = axum::http::Request::builder()
            .header("x-forwarded-prefix", "/ACC")
            .body(())
            .unwrap()
            .into_parts();
        parts
            .extensions
            .insert(RemoteAddr("127.0.0.1:5000".parse::<SocketAddr>().unwrap()));
        let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
        let cx = CxTestBuilder::new()
            .request_context(parts)
            .app_context(proxies)
            .build();
        let html = render(&cx, Some("lific:logo"), 15).await;
        assert!(html.contains("src=\"/ACC/logo.webp\""));
        assert!(html.contains("alt=\"Lific\""));
        assert!(html.contains("width: 15px; height: 15px;"));
        assert!(!html.contains("icon-192.png"));
    }
}
