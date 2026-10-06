//! Project icons follow the original master's approved icon and emoji data.

use std::{
    collections::{BTreeMap, HashSet},
    sync::LazyLock,
};

use futures_util::FutureExt;
use topcoat::{
    context::Cx,
    view::{Attributes, BoxView, ViewExt, view},
};

use super::{preloads::image_url, transport::mounted_url};
use crate::db::models::{Priority, Status};

mod ui;
pub(crate) use ui::UiIcon;

type IconNodes = Vec<(String, BTreeMap<String, String>)>;

pub(crate) fn stylesheet() -> &'static str {
    static CSS: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CSS.get_or_init(|| {
        let mut css = include_str!("assets/icons.css").to_owned();
        let context = Cx::default();
        let cx = &context;
        for (name, nodes) in ICONS.iter() {
            let nodes = icon_nodes(cx, nodes);
            let svg = view! { cx =>
                <svg xmlns="http://www.w3.org/2000/svg" id="icon" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    for (tag, attributes) in nodes { <(tag) (attributes)/> }
                </svg>
            }
            .single()
            .now_or_never()
            .expect("frozen icon geometry resolves synchronously")
            .expect("frozen icon geometry renders successfully")
            .render(cx);
            let controls = match name.as_str() {
                "ChevronRight" => ",.ns-project-toggle::before,.native-sidebar-group-toggle::before,.native-sidebar-mobile-project::after",
                "Ellipsis" => ",.ns-overflow::before,.native-sidebar-phone-actions::before",
                _ => "",
            };
            css.push_str(&format!(
                "\nsvg.native-icon[data-icon=\"{name}\"]{controls} {{ mask-image: url(\"data:image/svg+xml,{}\"); }}",
                urlencoding::encode(&svg)
            ));
        }
        css
    })
}

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

fn icon_nodes<'a>(cx: &Cx, nodes: &'a IconNodes) -> Vec<(&'a str, Attributes)> {
    nodes
        .iter()
        .map(|(tag, values)| {
            let mut attributes = Attributes::with_capacity(values.len());
            for (key, value) in values {
                attributes.insert(cx, key.as_str(), value.as_str());
            }
            (tag.as_str(), attributes)
        })
        .collect()
}

pub(crate) fn project_icon<'a>(cx: &'a Cx, value: Option<&str>, size: u32) -> BoxView<'a> {
    render_project_icon(cx, value, size, image_url)
}

/// Render a semantic UI icon using geometry embedded in the shared stylesheet.
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
        .filter(|name| ICONS.contains_key(*name))
        .unwrap_or("Folder");
    lucide_icon(cx, name, size)
}

fn lucide_icon<'a>(cx: &'a Cx, name: &str, size: u32) -> BoxView<'a> {
    let name = name.to_owned();
    view! { cx =>
        <svg class="native-icon" data-icon=(name) width=(size) height=(size) viewBox="0 0 24 24" aria-hidden="true">
            <rect width="24" height="24" fill="currentColor" stroke="none"></rect>
        </svg>
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
    fn complete_icon_geometry_is_embedded_in_the_shared_stylesheet_without_svg_requests() {
        let css = stylesheet();
        assert!(
            css.contains("data:image/svg+xml,"),
            "inline image data must replace external icon requests"
        );
        for name in [
            "Circle",
            "CircleDot",
            "Fan",
            "Pill",
            "Database",
            "Network",
            "Sigma",
        ] {
            assert!(
                css.contains(&format!("svg.native-icon[data-icon=\"{name}\"]")),
                "stylesheet glyph {name}"
            );
        }
        assert!(!css.contains("__native_icons/"));
        assert!(!css.contains(".mask.svg"));
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
            assert!(!html.contains("<path") && !html.contains("<circle"));
        }
    }

    #[tokio::test]
    async fn semantic_aliases_share_one_stylesheet_image() {
        let css = stylesheet();
        let cx = Cx::default();
        for icon in UiIcon::VARIANTS {
            let glyph = icon.glyph();
            assert!(ICONS.contains_key(glyph), "approved geometry for {icon:?}");
            assert_eq!(
                css.matches(&format!("svg.native-icon[data-icon=\"{glyph}\"]"))
                    .count(),
                1
            );
            let html = ui_icon(&cx, *icon, 16).single().await.unwrap().render(&cx);
            assert!(html.contains(&format!("data-icon=\"{glyph}\"")));
            assert!(!html.contains("<use") && !html.contains("href="));
        }
        assert!(std::ptr::eq(css, stylesheet()), "render stylesheet once");
        for controls in [
            ",.ns-project-toggle::before,.native-sidebar-group-toggle::before,.native-sidebar-mobile-project::after",
            ",.ns-overflow::before,.native-sidebar-phone-actions::before",
        ] {
            assert_eq!(
                css.matches(controls).count(),
                1,
                "controls share the glyph's image"
            );
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
        assert!(!html.contains("<circle "));
        assert!(!html.contains("lucide:Circle"));
    }

    #[tokio::test]
    async fn every_approved_icon_uses_a_compact_named_mask() {
        let cx = Cx::default();
        for name in ICONS.keys() {
            let html = render(&cx, Some(&format!("lucide:{name}")), 15).await;
            assert!(html.starts_with("<svg class=\"native-icon\""));
            assert!(html.contains(&format!("data-icon=\"{name}\"")));
            assert!(html.contains(
                "<rect width=\"24\" height=\"24\" fill=\"currentColor\" stroke=\"none\"></rect>"
            ));
            assert!(
                !html.contains("href=") && !html.contains("data:image") && !html.contains("style=")
            );
            assert!(!html.contains("<path") && !html.contains("<circle"));
            assert!(html.len() < 250, "compact instance for {name}");
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
