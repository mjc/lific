//! Project icons follow the original master's approved icon and emoji data.

use std::{
    collections::{BTreeMap, HashSet},
    sync::LazyLock,
};

use topcoat::{
    context::Cx,
    router::{Body, path_param, response::Response, route},
    view::{Attributes, BoxView, ViewExt, view},
};

use super::transport::mounted_url;
use crate::db::models::{Priority, Status};

type IconNodes = Vec<(String, BTreeMap<String, String>)>;

pub(crate) const STYLESHEET: &str = include_str!("assets/icons.css");

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

// The version includes the renderer revision so cached geometry changes with its format.
static ICON_VERSION: LazyLock<String> = LazyLock::new(|| {
    let mut hash = blake3::Hasher::new();
    hash.update(b"native-icon-asset-v1");
    hash.update(include_bytes!("assets/project-icons.json"));
    hash.finalize().to_hex().to_string()
});
static ICON_ASSETS: LazyLock<BTreeMap<String, tokio::sync::OnceCell<String>>> =
    LazyLock::new(|| {
        ICONS
            .keys()
            .map(|name| (name.clone(), tokio::sync::OnceCell::new()))
            .collect()
    });

path_param!(icon_version);
path_param!(icon_file);

#[route(GET "/__native_icons/{icon_version}/{icon_file}")]
async fn icon_asset(cx: &Cx) -> topcoat::Result<Response> {
    let version = path_param::<IconVersion>(cx);
    let file = path_param::<IconFile>(cx);
    let name = file.strip_suffix(".svg").unwrap_or("");
    if version != ICON_VERSION.as_str() || !ICONS.contains_key(name) {
        return Ok(Response::builder().status(404).body(Body::empty())?);
    }
    let svg = icon_svg(name).await?;
    Ok(Response::builder()
        .header("content-type", "image/svg+xml")
        .header("cache-control", "public, max-age=31536000, immutable")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(svg.as_bytes()))?)
}

async fn icon_svg(name: &str) -> topcoat::Result<&'static str> {
    let cache = ICON_ASSETS
        .get(name)
        .expect("caller checked the icon allowlist");
    let svg = cache
        .get_or_try_init(|| async {
            let context = Cx::default();
            let cx = &context;
            let nodes: Vec<_> = ICONS
                .get(name)
                .expect("approved icon")
                .iter()
                .map(|(tag, values)| {
                    let mut attributes = Attributes::with_capacity(values.len());
                    for (key, value) in values {
                        attributes.insert(cx, key.as_str(), value.as_str());
                    }
                    (tag.as_str(), attributes)
                })
                .collect();
            let html = view! { cx =>
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                    <g id="icon">
                        for (tag, attributes) in nodes { <(tag) (attributes)/> }
                    </g>
                </svg>
            }
            .single()
            .await?
            .render(cx);
            Ok::<_, topcoat::Error>(html)
        })
        .await?;
    Ok(svg.as_str())
}

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
    let name = value
        .strip_prefix("lucide:")
        .filter(|name| ICONS.contains_key(*name))
        .unwrap_or("Folder");
    let href = mounted_url(
        cx,
        &format!("/__native_icons/{}/{name}.svg#icon", ICON_VERSION.as_str()),
    );
    view! { cx =>
        <svg class="native-icon" width=(size) height=(size) viewBox="0 0 24 24" aria-hidden="true">
            <use href=(href)></use>
        </svg>
    }
    .boxed()
}

pub(crate) fn status_icon(cx: &Cx, status: Status, size: u32) -> BoxView<'_> {
    let (icon, color) = match status {
        Status::Active => ("lucide:CircleDot", "var(--tc-accent)"),
        Status::Todo => ("lucide:Circle", "var(--tc-muted)"),
        Status::Done => ("lucide:CircleCheckBig", "var(--tc-success)"),
        Status::Backlog => ("lucide:CircleDashed", "var(--tc-faint)"),
        Status::Cancelled => ("lucide:CircleX", "var(--tc-faint)"),
    };
    view! { cx =>
        <span data-status=(status.to_string()) style=(format!("color:{color};display:inline-flex;flex-shrink:0"))>
            (project_icon(cx, Some(icon), size))
        </span>
    }.boxed()
}

pub(crate) fn priority_icon(cx: &Cx, priority: Priority, size: u32) -> BoxView<'_> {
    let (color, bars): (_, &[u8]) = match priority {
        Priority::Urgent => ("var(--tc-danger)", &[]),
        Priority::High => ("var(--tc-warn)", &[12, 6, 18]),
        Priority::Medium => ("var(--tc-accent)", &[9, 15]),
        Priority::Low => ("var(--tc-muted)", &[12]),
        Priority::None => ("inherit", &[]),
    };
    view! { cx =>
        <svg width=(size) height=(size) viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" data-priority=(priority.to_string()) style=(format!("color:{color};flex-shrink:0"))>
            if matches!(priority, Priority::Urgent) {
                <circle cx="12" cy="12" r="10"></circle>
                <line x1="12" y1="8" x2="12" y2="12"></line><line x1="12" y1="16" x2="12.01" y2="16"></line>
            }
            for y in bars { <line x1="5" y1=(y.to_string()) x2="19" y2=(y.to_string())></line> }
        </svg>
    }.boxed()
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
    async fn approved_lucide_references_shared_geometry_at_requested_size() {
        let html = render(&Cx::default(), Some("lucide:Circle"), 19).await;
        for attribute in [
            "width=\"19\"",
            "height=\"19\"",
            "viewBox=\"0 0 24 24\"",
            "aria-hidden=\"true\"",
        ] {
            assert!(html.contains(attribute), "missing {attribute}: {html}");
        }
        assert!(html.contains("<use href=\"/__native_icons/"), "{html}");
        assert!(html.contains("/Circle.svg#icon\""), "{html}");
        assert!(!html.contains("<circle "));
        assert!(!html.contains("lucide:Circle"));
    }

    #[tokio::test]
    async fn project_svg_uses_shared_presentation_class_without_repeating_inline_styles() {
        let html = render(&Cx::default(), Some("lucide:Circle"), 19).await;
        assert!(html.contains("class=\"native-icon\""), "{html}");
        for repeated in [
            "fill=",
            "stroke=",
            "stroke-width=",
            "stroke-linecap=",
            "stroke-linejoin=",
            "style=",
        ] {
            assert!(!html.contains(repeated), "repeated {repeated}: {html}");
        }
    }

    #[tokio::test]
    async fn every_approved_lucide_uses_versioned_selected_asset_without_repeating_geometry() {
        let cx = Cx::default();
        let mut asset = None;
        for name in ICONS.keys() {
            let html = render(&cx, Some(&format!("lucide:{name}")), 15).await;
            let href = html
                .split("href=\"")
                .nth(1)
                .expect("shared asset reference")
                .split('"')
                .next()
                .unwrap();
            let (path, fragment) = href.split_once('#').unwrap();
            assert_eq!(fragment, "icon");
            assert!(path.ends_with(&format!("/{name}.svg")));
            let catalog = path.rsplit_once('/').unwrap().0;
            assert_eq!(asset.get_or_insert_with(|| catalog.to_owned()), catalog);
            assert!(!html.contains("<path ") && !html.contains("<circle "));
        }
    }

    #[tokio::test]
    async fn selected_assets_preserve_every_approved_geometry_and_cache_the_result() {
        let shapes = scraper::Selector::parse("#icon > *").unwrap();
        for (name, nodes) in ICONS.iter() {
            let svg = icon_svg(name).await.unwrap();
            assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
            assert!(svg.contains("<g id=\"icon\">"));
            let document = scraper::Html::parse_document(svg);
            let actual: IconNodes = document
                .select(&shapes)
                .map(|node| {
                    let element = node.value();
                    let attributes = element
                        .attrs()
                        .map(|(key, value)| (key.to_owned(), value.to_owned()))
                        .collect();
                    (element.name().to_owned(), attributes)
                })
                .collect();
            assert_eq!(&actual, nodes, "original {name} geometry must be preserved");
            assert!(std::ptr::eq(svg, icon_svg(name).await.unwrap()));
            assert_eq!(svg.matches("<g ").count(), 1);
            assert!(!svg.contains("<script") && !svg.contains("<use"));
        }
        let circle = icon_svg("Circle").await.unwrap();
        for attribute in ["cx=\"12\"", "cy=\"12\"", "r=\"10\""] {
            assert!(circle.contains(attribute));
        }
        assert!(
            circle.len() < 300,
            "selected asset includes unrelated icons"
        );
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
        let circle = render(&cx, Some("lucide:Circle"), 15).await;
        assert!(circle.contains("href=\"/ACC/__native_icons/"), "{circle}");
        assert!(circle.contains("/Circle.svg#icon\""), "{circle}");
    }
}
