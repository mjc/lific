//! Project icons follow the original master's approved icon and emoji data.

use std::{
    collections::{BTreeMap, HashSet},
    sync::LazyLock,
};

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, ViewHandle, view},
};

use super::{preloads::image_url, transport::mounted_url};
use crate::db::models::{Priority, Status};

mod ui;
pub(crate) use ui::UiIcon;

#[cfg(test)]
type IconNodes = Vec<(String, BTreeMap<String, String>)>;

pub(crate) fn stylesheet() -> &'static str {
    include_str!("assets/icons.css")
}

// These are checked-in package data, never markup supplied by a project.
// See assets/icons.LICENSE.txt for the original versions and provenance.
#[cfg(test)]
static SOURCE_ICONS: LazyLock<BTreeMap<String, IconNodes>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("assets/project-icons.json"))
        .expect("the frozen original Lucide icon data is valid")
});
static ICON_BODIES: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("assets/project-icons.inline.json"))
        .expect("the SVGO-optimized inline Lucide icon data is valid")
});
static EMOJI: LazyLock<HashSet<String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("assets/emoji.json"))
        .expect("the frozen original emoji allowlist is valid")
});

pub(crate) fn project_icon<'a>(cx: &'a Cx, value: Option<&str>, size: u32) -> BoxView<'a> {
    render_project_icon(cx, value, size, image_url)
}

/// Render a semantic UI icon using its full inline SVG geometry.
pub(crate) fn ui_icon(cx: &Cx, icon: UiIcon, size: u32) -> BoxView<'_> {
    lucide_icon(cx, icon.glyph(), size)
}

/// Unselected picker choices do not contribute initial document preload hints.
pub(crate) fn picker_choice_icon<'a>(cx: &'a Cx, value: Option<&str>, size: u32) -> BoxView<'a> {
    render_project_icon(cx, value, size, mounted_url)
}

fn render_project_icon<'a>(
    cx: &'a Cx,
    value: Option<&str>,
    size: u32,
    asset_url: fn(&Cx, &str) -> String,
) -> BoxView<'a> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return view! { cx => "" }.boxed();
    };
    if value == "lific:logo" {
        let src = asset_url(cx, "/logo.webp");
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
        .filter(|name| ICON_BODIES.contains_key(*name))
        .unwrap_or("Folder");
    lucide_icon(cx, name, size)
}

fn lucide_icon<'a>(cx: &'a Cx, name: &str, size: u32) -> BoxView<'a> {
    let name = name.to_owned();
    let body = ICON_BODIES
        .get(&name)
        .unwrap_or_else(|| panic!("missing approved inline icon geometry: {name}"));
    // Only the checked-in SVGO output reaches this raw-markup boundary.
    let body = ViewHandle::unescaped_unchecked(body);
    view! { cx =>
        <svg class="native-icon" data-icon=(name) width=(size) height=(size) viewBox="0 0 24 24" aria-hidden="true">(body)</svg>
    }.boxed()
}

pub(crate) fn status_icon(cx: &Cx, status: Status, size: u32) -> BoxView<'_> {
    let (icon, color) = match status {
        Status::Active => (UiIcon::ActiveIssue, "var(--tc-accent)"),
        Status::Todo => (UiIcon::TodoIssue, "var(--tc-muted)"),
        Status::Done => (UiIcon::DoneIssue, "var(--tc-success)"),
        Status::Backlog => (UiIcon::BacklogIssue, "var(--tc-faint)"),
        Status::Cancelled => (UiIcon::CancelledIssue, "var(--tc-faint)"),
    };
    view! { cx =>
        <span data-status=(status.to_string()) style=(format!("color:{color};display:inline-flex;flex-shrink:0"))>
            (ui_icon(cx, icon, size))
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

    use strum::VariantArray;
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

    #[test]
    fn icon_stylesheet_contains_no_embedded_image_catalog_or_external_icon_route() {
        let css = stylesheet();
        assert!(!css.contains("data:image/svg+xml,"));
        assert!(!css.contains("mask-image"));
        assert!(!css.contains("__native_icons/"));
        assert!(!css.contains(".mask.svg"));
    }

    #[test]
    fn optimized_inline_map_uses_self_closing_shapes_and_reduces_catalog_bytes() {
        let source_bytes = include_str!("assets/project-icons.json").len();
        let optimized_bytes = include_str!("assets/project-icons.inline.json").len();
        assert!(optimized_bytes < source_bytes * 95 / 100);
        let bodies = include_str!("assets/project-icons.inline.json");
        assert!(bodies.contains("<path ") && bodies.contains("/>"));
        assert!(!bodies.contains("</path>") && !bodies.contains("</circle>"));
    }

    #[tokio::test]
    async fn semantic_ui_icons_share_geometry_and_preserve_requested_size() {
        let cx = Cx::default();
        for (icon, glyph) in [
            (UiIcon::Search, "Search"),
            (UiIcon::MoreActions, "Ellipsis"),
            (UiIcon::ShowPassword, "Eye"),
            (UiIcon::Preview, "Eye"),
            (UiIcon::ActiveIssue, "CircleDot"),
        ] {
            let html = ui_icon(&cx, icon, 19).single().await.unwrap().render(&cx);
            assert_eq!(
                html,
                render(&cx, Some(&format!("lucide:{glyph}")), 19).await
            );
            assert!(
                html.contains(&format!("data-icon=\"{glyph}\"")),
                "{icon:?}: {html}"
            );
            assert!(html.contains("width=\"19\"") && html.contains("height=\"19\""));
            assert!(html.contains("<path") || html.contains("<circle"));
        }
    }

    #[tokio::test]
    async fn semantic_aliases_render_their_shared_inline_geometry() {
        let cx = Cx::default();
        for icon in UiIcon::VARIANTS {
            let glyph = icon.glyph();
            assert!(
                SOURCE_ICONS.contains_key(glyph),
                "approved geometry for {icon:?}"
            );
            let html = ui_icon(&cx, *icon, 16).single().await.unwrap().render(&cx);
            assert!(html.contains(&format!("data-icon=\"{glyph}\"")));
            assert!(html.contains("<path") || html.contains("<circle") || html.contains("<rect"));
            assert!(!html.contains("<use") && !html.contains("href=") && !html.contains("mask"));
        }
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
        assert!(html.contains("data-icon=\"Circle\""), "{html}");
        assert!(!html.contains("<use") && !html.contains("href="));
        assert!(html.contains("<circle "));
        assert!(!html.contains("lucide:Circle"));
    }

    #[tokio::test]
    async fn every_approved_icon_renders_its_complete_frozen_geometry_inline() {
        let cx = Cx::default();
        for name in SOURCE_ICONS.keys() {
            let html = render(&cx, Some(&format!("lucide:{name}")), 15).await;
            assert!(html.starts_with("<svg class=\"native-icon\""));
            assert!(html.contains(&format!("data-icon=\"{name}\"")));
            assert!(!html.contains("href=") && !html.contains("data:image"));
            let document = scraper::Html::parse_fragment(&html);
            let selector = scraper::Selector::parse("svg > *").unwrap();
            let actual = document
                .select(&selector)
                .map(|element| {
                    (
                        element.value().name().to_owned(),
                        element
                            .value()
                            .attrs()
                            .map(|(key, value)| (key.to_owned(), value.to_owned()))
                            .collect::<BTreeMap<_, _>>(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, SOURCE_ICONS[name], "optimized geometry for {name}");
        }
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
        assert!(circle.contains("data-icon=\"Circle\""), "{circle}");
        assert!(
            !circle.contains("href="),
            "glyph selection is independent of mount"
        );
    }
}
