//! Project icons follow the original master's approved icon and emoji data.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::LazyLock,
};

use strum::VariantArray;
use topcoat::{
    context::Cx,
    router::{Body, path_param, response::Response, route},
    view::{Attributes, BoxView, ViewExt, view},
};

use super::{preloads::image_url, transport::mounted_url};
use crate::db::models::{Priority, Status};

mod ui;
pub(crate) use ui::UiIcon;

type IconNodes = Vec<(String, BTreeMap<String, String>)>;

// Semantic aliases share one symbol; arbitrary picker icons stay separate.
static UI_ICON_NAMES: LazyLock<BTreeSet<&'static str>> =
    LazyLock::new(|| UiIcon::VARIANTS.iter().map(|icon| icon.glyph()).collect());

pub(crate) fn stylesheet() -> &'static str {
    static CSS: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CSS.get_or_init(|| {
        let mut css = include_str!("assets/icons.css").to_owned();
        for (name, selector) in [
            ("ChevronRight", ".ns-project-toggle::before,.native-sidebar-group-toggle::before,.native-sidebar-mobile-project::after"),
            ("Ellipsis", ".ns-overflow::before,.native-sidebar-phone-actions::before"),
        ] {
            css.push_str(&format!(
                "\n{selector} {{ mask-image: url(\"__native_icons/{}/{name}.mask.svg\"); }}",
                ICON_VERSION.as_str()
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

// The version includes the renderer revision so cached geometry changes with its format.
static ICON_VERSION: LazyLock<String> = LazyLock::new(|| icon_version(&UI_ICON_NAMES));

fn icon_version(names: &BTreeSet<&str>) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(b"native-icon-asset-v3-ui-sprite");
    hash.update(include_bytes!("assets/project-icons.json"));
    for name in names {
        hash.update(name.as_bytes());
        hash.update(b"\0");
    }
    hash.finalize().to_hex().to_string()
}
static ICON_ASSETS: LazyLock<BTreeMap<String, tokio::sync::OnceCell<String>>> =
    LazyLock::new(|| {
        ICONS
            .keys()
            .map(|name| (name.clone(), tokio::sync::OnceCell::new()))
            .collect()
    });
static MASK_ASSETS: LazyLock<BTreeMap<String, tokio::sync::OnceCell<String>>> =
    LazyLock::new(|| {
        ["ChevronRight", "Ellipsis"]
            .into_iter()
            .map(|name| (name.to_owned(), tokio::sync::OnceCell::new()))
            .collect()
    });

path_param!(icon_version);
path_param!(icon_file);

#[route(GET "/__native_icons/{icon_version}/{icon_file}")]
async fn icon_asset(cx: &Cx) -> topcoat::Result<Response> {
    let version = path_param::<IconVersion>(cx);
    let file = path_param::<IconFile>(cx);
    let (name, mask) = match file.strip_suffix(".mask.svg") {
        Some(name) => (name, true),
        None => (file.strip_suffix(".svg").unwrap_or(""), false),
    };
    let allowed = if mask {
        MASK_ASSETS.contains_key(name)
    } else {
        file == "ui.svg" || ICONS.contains_key(name)
    };
    if version != ICON_VERSION.as_str() || !allowed {
        return Ok(Response::builder().status(404).body(Body::empty())?);
    }
    let svg = if file == "ui.svg" {
        ui_sprite().await?
    } else if mask {
        rendered_icon(name, true).await?
    } else {
        icon_svg(name).await?
    };
    Ok(Response::builder()
        .header("content-type", "image/svg+xml")
        .header("cache-control", "public, max-age=31536000, immutable")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(svg.as_bytes()))?)
}

async fn icon_svg(name: &str) -> topcoat::Result<&'static str> {
    rendered_icon(name, false).await
}

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

async fn ui_sprite() -> topcoat::Result<&'static str> {
    static SPRITE: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    let svg = SPRITE
        .get_or_try_init(|| async {
            let context = Cx::default();
            let cx = &context;
            let symbols: Vec<_> = UI_ICON_NAMES
                .iter()
                .map(|name| {
                    (
                        *name,
                        icon_nodes(cx, ICONS.get(*name).expect("approved UI icon")),
                    )
                })
                .collect();
            let svg = view! { cx =>
                <svg xmlns="http://www.w3.org/2000/svg">
                    for (name, nodes) in symbols {
                        <symbol id=(name) viewBox="0 0 24 24">
                            for (tag, attributes) in nodes { <(tag) (attributes)/> }
                        </symbol>
                    }
                </svg>
            }
            .single()
            .await?
            .render(cx);
            Ok::<_, topcoat::Error>(svg)
        })
        .await?;
    Ok(svg.as_str())
}

async fn rendered_icon(name: &str, mask: bool) -> topcoat::Result<&'static str> {
    let assets = if mask { &MASK_ASSETS } else { &ICON_ASSETS };
    let cache = assets.get(name).expect("caller checked the icon allowlist");
    let svg = cache
        .get_or_try_init(|| async {
            let context = Cx::default();
            let cx = &context;
            let mut presentation = Attributes::default();
            if mask {
                for (key, value) in [
                    ("stroke", "black"),
                    ("fill", "none"),
                    ("stroke-width", "2"),
                    ("stroke-linecap", "round"),
                    ("stroke-linejoin", "round"),
                ] {
                    presentation.insert(cx, key, value);
                }
            }
            let nodes = icon_nodes(cx, ICONS.get(name).expect("approved icon"));
            let html = view! { cx =>
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" (presentation)>
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
    render_project_icon(cx, value, size, image_url)
}

/// Render a semantic UI icon from the shared sprite and preload it on initial load.
pub(crate) fn ui_icon(cx: &Cx, icon: UiIcon, size: u32) -> BoxView<'_> {
    lucide_icon(cx, "ui.svg", icon.glyph(), size, image_url)
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
    let (file, fragment) = if UI_ICON_NAMES.contains(&name) {
        ("ui.svg".to_owned(), name)
    } else {
        (format!("{name}.svg"), "icon")
    };
    lucide_icon(cx, &file, fragment, size, asset_url)
}

fn lucide_icon<'a>(
    cx: &'a Cx,
    file: &str,
    fragment: &str,
    size: u32,
    asset_url: fn(&Cx, &str) -> String,
) -> BoxView<'a> {
    let href = format!(
        "{}#{fragment}",
        asset_url(
            cx,
            &format!("/__native_icons/{}/{file}", ICON_VERSION.as_str()),
        )
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
                html.contains(&format!("/ui.svg#{glyph}\"")),
                "{icon:?}: {html}"
            );
            assert!(html.contains("width=\"19\"") && html.contains("height=\"19\""));
            assert!(!html.contains("<path") && !html.contains("<circle"));
        }
    }

    #[test]
    fn sprite_version_changes_with_glyph_membership() {
        let one = BTreeSet::from(["Circle"]);
        let two = BTreeSet::from(["Circle", "CircleDot"]);
        assert_ne!(icon_version(&one), icon_version(&two));
        assert_eq!(
            icon_version(&two),
            icon_version(&BTreeSet::from(["CircleDot", "Circle"]))
        );
    }

    #[tokio::test]
    async fn every_semantic_icon_has_one_approved_sprite_symbol() {
        let svg = ui_sprite().await.unwrap();
        let document = scraper::Html::parse_document(svg);
        let symbols = scraper::Selector::parse("symbol[id]").unwrap();
        let names: BTreeSet<_> = document
            .select(&symbols)
            .map(|node| node.value().attr("id").unwrap())
            .collect();
        assert_eq!(names, *UI_ICON_NAMES);
        assert_eq!(
            document.select(&symbols).count(),
            names.len(),
            "aliases share one geometry definition"
        );
        assert!(
            UiIcon::VARIANTS.len() > names.len(),
            "semantic aliases are present"
        );
        let cx = Cx::default();
        for icon in UiIcon::VARIANTS {
            let glyph = icon.glyph();
            assert!(ICONS.contains_key(glyph), "approved geometry for {icon:?}");
            assert!(names.contains(glyph), "registered symbol for {icon:?}");
            let html = ui_icon(&cx, *icon, 16).single().await.unwrap().render(&cx);
            assert!(
                html.contains(&format!("/ui.svg#{glyph}\"")),
                "{icon:?}: {html}"
            );
        }
        assert!(
            std::ptr::eq(svg, ui_sprite().await.unwrap()),
            "cache rendered sprite once"
        );
    }

    #[tokio::test]
    async fn fixed_ui_icons_share_one_immutable_sprite_with_original_geometry() {
        use tower::ServiceExt;

        let cx = Cx::default();
        let mut paths = std::collections::BTreeSet::new();
        for name in ["Circle", "CircleDot", "Plus", "Folder"] {
            let html = render(&cx, Some(&format!("lucide:{name}")), 19).await;
            let document = scraper::Html::parse_document(&html);
            let node = document
                .select(&scraper::Selector::parse("svg.native-icon > use").unwrap())
                .next()
                .unwrap();
            let (path, fragment) = node.value().attr("href").unwrap().split_once('#').unwrap();
            assert!(path.ends_with("/ui.svg"), "shared UI sprite: {html}");
            assert_eq!(fragment, name);
            assert!(!html.contains("<circle") && !html.contains("<path"));
            paths.insert(path.to_owned());
        }
        assert_eq!(paths.len(), 1, "UI icons share a single download");
        let fixture = super::super::home_fixture::fixture();
        let response = fixture
            .app
            .oneshot(
                axum::http::Request::builder()
                    .uri(paths.first().unwrap())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "image/svg+xml");
        assert_eq!(
            response.headers()["cache-control"],
            "public, max-age=31536000, immutable"
        );
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(bytes.len() < 16_000, "fixed UI set, not the picker catalog");
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(!svg.contains("<script") && !svg.contains("<use"));
        let document = scraper::Html::parse_document(svg);
        let symbols = scraper::Selector::parse("symbol").unwrap();
        let shapes = scraper::Selector::parse(":scope > *").unwrap();
        let mut names = std::collections::BTreeSet::new();
        for symbol in document.select(&symbols) {
            let name = symbol.value().attr("id").unwrap();
            assert_eq!(symbol.value().attr("viewBox"), Some("0 0 24 24"));
            let actual: IconNodes = symbol
                .select(&shapes)
                .map(|node| {
                    let element = node.value();
                    (
                        element.name().to_owned(),
                        element
                            .attrs()
                            .map(|(key, value)| (key.to_owned(), value.to_owned()))
                            .collect(),
                    )
                })
                .collect();
            assert_eq!(
                &actual,
                ICONS.get(name).unwrap(),
                "original {name} geometry"
            );
            assert!(names.insert(name.to_owned()), "unique symbol {name}");
        }
        for name in ["Circle", "CircleDot", "Plus", "Folder"] {
            assert!(names.contains(name));
        }
        assert!(
            !names.contains("AArrowDown"),
            "arbitrary picker icons stay separate"
        );
        assert!(
            render(&cx, Some("lucide:AArrowDown"), 19)
                .await
                .contains("/AArrowDown.svg#icon")
        );
    }

    #[tokio::test]
    async fn repeated_sidebar_icons_use_compact_masks_with_original_immutable_geometry() {
        use tower::ServiceExt;

        let fixture = super::super::home_fixture::fixture();
        let stylesheet = super::super::super::assets::app_stylesheet();
        for (name, selector) in [
            ("ChevronRight", ".ns-project-toggle::before"),
            ("Ellipsis", ".ns-overflow::before"),
        ] {
            assert!(
                stylesheet.contains(selector),
                "Fixed control mask selector {selector}"
            );
            let asset = format!("__native_icons/{}/{name}.mask.svg", ICON_VERSION.as_str());
            assert!(
                stylesheet.contains(&asset),
                "shared mask reference for {name}"
            );
            assert!(
                !stylesheet.contains(&format!("url(\"/{asset}")),
                "Mask URLs resolve relative to the mounted stylesheet"
            );
            let response = fixture
                .app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri(format!("/{asset}"))
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
            assert_eq!(
                response.headers()["cache-control"],
                "public, max-age=31536000, immutable"
            );
            let svg = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let svg = std::str::from_utf8(&svg).unwrap();
            for attribute in [
                "stroke=\"black\"",
                "fill=\"none\"",
                "stroke-width=\"2\"",
                "stroke-linecap=\"round\"",
                "stroke-linejoin=\"round\"",
            ] {
                assert!(
                    svg.contains(attribute),
                    "mask presentation {attribute}: {svg}"
                );
            }
            let document = scraper::Html::parse_document(svg);
            let actual: IconNodes = document
                .select(&scraper::Selector::parse("#icon > *").unwrap())
                .map(|node| {
                    let element = node.value();
                    (
                        element.name().to_owned(),
                        element
                            .attrs()
                            .map(|(key, value)| (key.to_owned(), value.to_owned()))
                            .collect(),
                    )
                })
                .collect();
            assert_eq!(&actual, ICONS.get(name).unwrap());
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
        assert!(html.contains("<use href=\"/__native_icons/"), "{html}");
        assert!(html.contains("/ui.svg#Circle\""), "{html}");
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
            assert!(html.starts_with("<svg "), "Generic Lucide icon: {html}");
            let href = html
                .split("href=\"")
                .nth(1)
                .expect("shared asset reference")
                .split('"')
                .next()
                .unwrap();
            let (path, fragment) = href.split_once('#').unwrap();
            if UI_ICON_NAMES.contains(&name.as_str()) {
                assert_eq!(fragment, name);
                assert!(path.ends_with("/ui.svg"));
            } else {
                assert_eq!(fragment, "icon");
                assert!(path.ends_with(&format!("/{name}.svg")));
            }
            let catalog = path.rsplit_once('/').unwrap().0;
            let catalog = catalog.trim_start_matches('/');
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
        assert!(circle.contains("/ui.svg#Circle\""), "{circle}");
    }
}
