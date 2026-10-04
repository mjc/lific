use std::{net::SocketAddr, sync::Arc};

use topcoat::{
    context::{Cx, CxTestBuilder},
    router::RemoteAddr,
};

use super::{Scope, render};
use crate::ratelimit::IpNetwork;

fn context(prefix: &str) -> Cx {
    let (mut parts, ()) = axum::http::Request::builder()
        .header("x-forwarded-prefix", prefix)
        .body(())
        .unwrap()
        .into_parts();
    parts
        .extensions
        .insert(RemoteAddr("127.0.0.1:3000".parse::<SocketAddr>().unwrap()));
    let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
    CxTestBuilder::new()
        .app_context(proxies)
        .request_context(parts)
        .build()
}

fn private(source: &str) -> String {
    render(&Cx::default(), source, Scope::Private, &[])
}

#[test]
fn native_markdown_master_renders_headings_prose_emphasis_and_hard_line_breaks() {
    // Characterizes marked.parse(..., {breaks:true,gfm:true}).
    let html = private("# Heading\n\nFirst **bold** and *italic*.\nNext line.\n\n~~removed~~");
    assert!(html.contains("<h1>Heading</h1>"));
    assert!(html.contains("<strong>bold</strong>"));
    assert!(html.contains("<em>italic</em>"));
    assert!(html.contains(".<br>\nNext line."), "{html}");
    assert!(html.contains("<del>removed</del>"));
    assert!(!html.contains("<pre>"));
}

#[test]
fn native_markdown_master_normalizes_literal_escaped_newlines_before_parsing() {
    assert_eq!(private("# Heading\\n\\nBody"), private("# Heading\n\nBody"));
}

#[test]
fn native_markdown_master_nested_lists_keep_children_inside_the_parent_item() {
    // Same concrete sources as the existing public Markdown DOM parity corpus.
    for (source, expected) in [
        (
            "- Parent\n  - Child",
            "<ul><li>Parent<ul><li>Child</li></ul></li></ul>",
        ),
        (
            "1. Parent\n   - Child\n2. Sibling",
            "<ol><li>Parent<ul><li>Child</li></ul></li><li>Sibling</li></ol>",
        ),
        (
            "10. Parent\n    - Child\n11. Sibling",
            "<ol start=\"10\"><li>Parent<ul><li>Child</li></ul></li><li>Sibling</li></ol>",
        ),
    ] {
        // Only formatter newlines differ here; all nesting and text remain exact.
        assert_eq!(private(source).replace('\n', ""), expected, "{source}");
    }
}

#[test]
fn native_markdown_master_gfm_tables_and_task_checkboxes_survive_sanitizing() {
    let html = private(
        "| Name | Count |\n| --- | --- |\n| Widget | 2 |\nParagraph immediately after the table.",
    );
    for markup in [
        "<table>",
        "<thead>",
        "<th>Name</th>",
        "<th>Count</th>",
        "<tbody>",
        "<td>Widget</td>",
        "<td>2</td>",
        "</table>",
    ] {
        assert!(html.contains(markup), "missing {markup}: {html}");
    }
    // The pinned marked parser treats this line as a table row without a
    // separating blank line. Keep that exact tree, including the empty cell.
    assert!(
        html.contains("<td>Paragraph immediately after the table.</td>"),
        "{html}"
    );
    assert!(!html.contains("<p>Paragraph immediately after the table.</p>"));
    assert_eq!(html.matches("<tr>").count(), 3, "{html}");
    let html = private("- [x] Done\n- [ ] Not done");
    let inputs: Vec<_> = html
        .split("<input")
        .skip(1)
        .map(|part| part.split('>').next().unwrap())
        .collect();
    assert_eq!(inputs.len(), 2, "{html}");
    assert!(
        inputs
            .iter()
            .all(|input| input.contains("type=\"checkbox\"") && input.contains("disabled=\"\""))
    );
    assert!(inputs[0].contains("checked=\"\""));
    assert!(!inputs[1].contains("checked="));
}

#[test]
fn native_markdown_master_code_preserves_language_and_escaping_without_linkification() {
    let html = private("`ACC-2 #91 @viewer`\n\n```js\nACC-2 #91 <script> &\n```");
    assert!(html.contains("<code>ACC-2 #91 @viewer</code>"));
    assert!(html.contains("class=\"code-block-wrapper\""));
    assert!(html.contains("data-lang=\"js\""));
    assert!(html.contains(
        "<pre><code class=\"language-js\">ACC-2 #91 &lt;script&gt; &amp;\n</code></pre>"
    ));
    assert!(!html.contains("<a "));
    assert!(!html.contains("<script>"));
}

#[test]
fn native_markdown_master_explicit_links_and_gfm_autolinks_keep_safe_destinations() {
    let html = private(
        "[**Rust**](https://example.test/path?q=1) https://example.test/auto www.example.test reader@example.test",
    );
    assert!(html.contains("href=\"https://example.test/path?q=1\""));
    assert!(html.contains("<strong>Rust</strong></a>"));
    assert!(html.contains("href=\"https://example.test/auto\""));
    assert!(html.contains("href=\"http://www.example.test\""));
    assert!(html.contains("href=\"mailto:reader@example.test\""));
    assert!(
        !html.contains("<a href=\"mailto:reader@example.test\"><span"),
        "email is not a mention"
    );
}

#[test]
fn native_markdown_master_identifier_and_comment_routes_mount_once_without_nested_links() {
    for prefix in ["", "/app", "/ACC"] {
        let html = render(
            &context(prefix),
            "ACC-2 ACC-DOC-3 ACC-PLAN-7 ACC-2#comment-91 #91 `ACC-2` [**ACC-2**](#/ACC/issues/ACC-2)",
            Scope::Private,
            &[],
        );
        for route in [
            "/ACC/issues/ACC-2",
            "/ACC/pages",
            "/ACC/plans",
            "/ACC/issues/ACC-2?comment=91",
        ] {
            assert!(
                html.contains(&format!("href=\"{prefix}{route}\"")),
                "{html}"
            );
        }
        assert!(html.contains("href=\"#comment-91\""));
        assert!(html.contains("data-issue-ident=\"ACC-2\""));
        assert!(html.contains("<code>ACC-2</code>"));
        assert!(html.contains("<strong>ACC-2</strong></a>"));
        assert!(!html.contains("<strong><a"));
        assert!(!html.contains("<code><a"));
    }
}

#[test]
fn native_markdown_master_reference_boundaries_do_not_link_entities_or_single_letter_projects() {
    let html = private("don't A-1 lowercase-1 ABCDEF-1 ACC-2");
    assert_eq!(html.matches("data-issue-ident=").count(), 1, "{html}");
    assert!(!html.contains("href=\"#comment-39\""));
    assert!(html.contains("A-1 lowercase-1 ABCDEF-1"));
}

#[test]
fn native_markdown_master_mentions_use_only_supplied_visible_members_and_escape_display_names() {
    let html = render(
        &Cx::default(),
        "@Viewer @unknown foo@viewer `@viewer` [@viewer](https://example.test)",
        Scope::Private,
        &[("viewer", "Visible <Person> & name")],
    );
    assert_eq!(html.matches("data-mention=").count(), 1, "{html}");
    assert!(html.contains("data-mention=\"Viewer\""));
    assert!(html.contains("title=\"@Viewer\""));
    assert!(html.contains("@Visible &lt;Person&gt; &amp; name</span>"));
    assert!(html.contains("@unknown"));
    assert!(html.contains("<code>@viewer</code>"));
    assert!(html.contains(">@viewer</a>"));
    assert!(!html.contains("<Person>"));
}

#[test]
fn native_markdown_master_safe_raw_html_survives_and_active_html_is_sanitized() {
    let html = private(
        "<span title=\"safe\"><b>Kept</b></span>\n\n<img src=\"https://example.test/image.png\" onerror=\"alert(1)\"><script>alert(1)</script><svg onload=\"alert(1)\"></svg>\n\n[bad](javascript:alert%281%29)",
    );
    assert!(html.contains("<span title=\"safe\"><b>Kept</b></span>"));
    assert!(
        html.contains("src=\"https://example.test/image.png\""),
        "private scope retains normal safe remote images"
    );
    for active in ["onerror", "onload", "<script", "javascript:"] {
        assert!(
            !html.contains(active),
            "active HTML survived: {active}: {html}"
        );
    }
    assert!(html.contains("bad</a>"));
}

#[test]
fn native_markdown_master_attachment_images_and_download_hooks_mount_once() {
    for prefix in ["", "/app", "/ACC"] {
        let html = render(
            &context(prefix),
            "![inline](/api/attachments/31) [file](/api/attachments/32)",
            Scope::Private,
            &[],
        );
        assert!(
            html.contains(&format!("src=\"{prefix}/api/attachments/31\"")),
            "{html}"
        );
        assert!(html.contains("alt=\"inline\""));
        assert!(html.contains(&format!("href=\"{prefix}/api/attachments/32\"")));
        assert!(html.contains("data-attachment=\"\""));
        assert!(html.contains("class=\"attachment-chip\""));
        assert!(html.contains("download=\"\""));
    }
}

#[test]
fn native_markdown_public_scope_keeps_routes_and_attachment_hooks_without_foreign_resources() {
    // Existing public DOM/network proof uses this concrete hostile resource set.
    let source = "[**ACC-2**](#/ACC/issues/ACC-2) ACC-2#comment-91 #91 `ACC-2`\n\n![inline](/api/attachments/31) [file](/api/attachments/32)\n<img alt=\"raw\" src=\"/api/attachments/33\">\n<img alt=\"tracker\" src=\"https://tracker.test/pixel\" srcset=\"https://tracker.test/pixel 2x\" onerror=\"bad()\">\n<input type=\"image\" src=\"https://tracker.test/input\">\n<div style=\"background:url(https://tracker.test/style)\">Safe</div>\n<video poster=\"https://tracker.test/poster\" src=\"https://tracker.test/video\"></video>\n<svg><image href=\"https://tracker.test/svg\"></image></svg>\n<script>bad()</script>";
    for prefix in ["", "/app", "/ACC"] {
        let html = render(&context(prefix), source, Scope::Published, &[]);
        assert!(html.contains(&format!("href=\"{prefix}/public/ACC/issues/ACC-2\"")));
        assert!(html.contains(&format!(
            "href=\"{prefix}/public/ACC/issues/ACC-2?comment=91\""
        )));
        assert!(html.contains("href=\"#comment-91\""));
        assert!(html.contains("<strong>ACC-2</strong></a>"));
        assert!(html.contains("<code>ACC-2</code>"));
        // Public hooks defer browser resource work to the existing safe media
        // surface; author HTML itself must not start a fetch on insertion.
        assert!(html.contains("data-public-image=\"31\""));
        assert!(html.contains("data-public-image=\"33\""));
        assert!(html.contains("data-public-download=\"32\""));
        for forbidden in [
            " src=",
            " srcset=",
            " style=",
            " poster=",
            " onerror=",
            "<script",
            "<svg",
            "<video",
            "<input type=\"image\"",
        ] {
            assert!(
                !html.contains(forbidden),
                "public author resource survived: {forbidden}: {html}"
            );
        }
        assert!(
            html.contains("tracker"),
            "blocked images retain readable alt text"
        );
        assert!(html.contains("Safe"));
    }
}

#[test]
fn native_markdown_master_same_page_comments_keep_the_comment_ref_surface() {
    let html = private("#91 and `#92` and [#93](https://example.test)");
    let document = scraper::Html::parse_fragment(&html);
    let selector = scraper::Selector::parse("a.comment-ref[href=\"#comment-91\"]").unwrap();
    assert_eq!(document.select(&selector).count(), 1, "{html}");
    assert_eq!(html.matches("class=\"comment-ref\"").count(), 1);
    assert!(html.contains("<code>#92</code>"));
    assert!(html.contains(">#93</a>"));
}

#[test]
fn native_markdown_public_attachment_images_require_relative_instance_paths() {
    // Original public sanitizer accepts only /^\/api\/attachments\/\d+\/?$/.
    let source = "![local](/api/attachments/31) ![foreign](https://foreign.test/api/attachments/32) ![absolute](https://instance.test/api/attachments/33)";
    let html = render(&Cx::default(), source, Scope::Published, &[]);
    assert!(html.contains("data-public-image=\"31\""), "{html}");
    assert!(!html.contains("data-public-image=\"32\""), "{html}");
    assert!(!html.contains("data-public-image=\"33\""), "{html}");
    assert!(html.contains("foreign") && html.contains("absolute"));
    assert!(!html.contains(" src="));
}

#[test]
fn native_markdown_master_reference_boundaries_use_javascript_ascii_word_characters() {
    // Pinned 9683 references.ts IDENTIFIER_RE and Markdown.svelte
    // CROSS_COMMENT_RE/COMMENT_REF_RE use JS \b without Unicode-mode folding.
    // Thus an accented neighbor is outside [A-Za-z0-9_], even though Rust
    // regex's default Unicode word class includes it.
    let source = "éACC-2 ACC-3é #91é";
    let html = private(source);
    let document = scraper::Html::parse_fragment(&html);
    let links = document
        .select(&scraper::Selector::parse("a").unwrap())
        .map(|element| {
            (
                element.attr("href").unwrap().to_owned(),
                element.attr("class").unwrap().to_owned(),
                element.text().collect::<String>(),
                element.attr("data-issue-ident").map(str::to_owned),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        links,
        vec![
            (
                "/ACC/issues/ACC-2".into(),
                "identifier-link".into(),
                "ACC-2".into(),
                Some("ACC-2".into())
            ),
            (
                "/ACC/issues/ACC-3".into(),
                "identifier-link".into(),
                "ACC-3".into(),
                Some("ACC-3".into())
            ),
            (
                "#comment-91".into(),
                "comment-ref".into(),
                "#91".into(),
                None
            ),
        ],
        "original JS boundaries must link the same prose tokens: {html}"
    );
    let paragraph = document
        .select(&scraper::Selector::parse("p").unwrap())
        .next()
        .unwrap();
    assert_eq!(paragraph.text().collect::<String>(), source);
}

#[test]
fn native_markdown_master_reference_digits_are_ascii_and_preserve_the_unmatched_suffix() {
    // JS \d excludes U+0663. Original IDENTIFIER_RE leaves ACC-٣ literal;
    // COMMENT_REF_RE matches just #9 in #9٣ because JS \b ends after ASCII 9.
    let source = "ACC-٣ #9٣";
    let html = private(source);
    let document = scraper::Html::parse_fragment(&html);
    let links = document
        .select(&scraper::Selector::parse("a").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        links.len(),
        1,
        "non-ASCII digits must not enter identifiers: {html}"
    );
    assert_eq!(links[0].attr("href"), Some("#comment-9"));
    assert_eq!(links[0].attr("class"), Some("comment-ref"));
    assert_eq!(links[0].text().collect::<String>(), "#9");
    assert!(links[0].attr("data-issue-ident").is_none());
    let paragraph = document
        .select(&scraper::Selector::parse("p").unwrap())
        .next()
        .unwrap();
    assert_eq!(
        paragraph.text().collect::<String>(),
        source,
        "the unmatched Unicode suffix stays readable"
    );
}

#[test]
fn native_markdown_master_code_wrapper_keeps_the_full_lowercase_fence_info() {
    // Pinned Markdown.svelte lowercases token.lang in data-lang while marked's
    // default code renderer uses only its first word in the language-* class.
    let source = "```JS example\nACC-2 <tag> &\n```";
    let html = private(source);
    let document = scraper::Html::parse_fragment(&html);
    let wrappers = document
        .select(&scraper::Selector::parse("div.code-block-wrapper").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(wrappers.len(), 1, "one original code wrapper: {html}");
    assert_eq!(
        wrappers[0].attr("data-lang"),
        Some("js example"),
        "fence metadata is part of the original hook: {html}"
    );
    let code = wrappers[0]
        .select(&scraper::Selector::parse("pre > code").unwrap())
        .next()
        .unwrap();
    assert_eq!(code.attr("class"), Some("language-JS"));
    assert_eq!(code.text().collect::<String>(), "ACC-2 <tag> &\n");
    assert_eq!(
        document
            .select(&scraper::Selector::parse("a, tag").unwrap())
            .count(),
        0,
        "code stays inert and unlinked"
    );
}

fn assert_attachment_identifiers_use_ascii_digits(scope: Scope) {
    // Pinned Markdown.svelte ATTACHMENT_HREF_RE/PUBLIC_IMG_SRC_RE use JS \d,
    // so U+0663 is ordinary URL text, never an attachment identifier.
    let source = concat!(
        "<img alt=\"ASCII attachment\" src=\"/api/attachments/31\"> ",
        "<a href=\"/api/attachments/32\">ASCII file</a> ",
        "<img alt=\"Unicode image\" src=\"/api/attachments/٣\"> ",
        "<a href=\"/api/attachments/٣\">Unicode file</a>"
    );
    for prefix in ["", "/app", "/ACC"] {
        let html = render(&context(prefix), source, scope, &[]);
        let document = scraper::Html::parse_fragment(&html);
        let anchors = document
            .select(&scraper::Selector::parse("a").unwrap())
            .collect::<Vec<_>>();
        assert_eq!(anchors.len(), 2, "both original link labels remain: {html}");
        let ascii = anchors
            .iter()
            .find(|anchor| anchor.text().collect::<String>() == "ASCII file")
            .unwrap();
        assert_eq!(ascii.attr("class"), Some("attachment-chip"));
        assert_eq!(ascii.attr("download"), Some(""));
        assert_eq!(
            ascii.attr("href"),
            Some(format!("{prefix}/api/attachments/32").as_str())
        );
        let unicode = anchors
            .iter()
            .find(|anchor| anchor.text().collect::<String>() == "Unicode file")
            .unwrap();
        for attribute in [
            "class",
            "download",
            "data-attachment",
            "data-public-download",
        ] {
            assert!(
                unicode.attr(attribute).is_none(),
                "Unicode digits must not grant {attribute} attachment semantics: {html}"
            );
        }
        // Compare the actual DOM destination after URI decoding, allowing the
        // sanitizer serializer to encode non-ASCII URL characters.
        assert_eq!(
            urlencoding::decode(unicode.attr("href").unwrap()).unwrap(),
            "/api/attachments/٣"
        );
        match scope {
            Scope::Private => {
                assert_eq!(ascii.attr("data-attachment"), Some(""));
                assert!(ascii.attr("data-public-download").is_none());
                let images = document
                    .select(&scraper::Selector::parse("img").unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(images.len(), 2);
                let ascii_image = images
                    .iter()
                    .find(|image| image.attr("alt") == Some("ASCII attachment"))
                    .unwrap();
                assert_eq!(
                    ascii_image.attr("src"),
                    Some(format!("{prefix}/api/attachments/31").as_str())
                );
                let unicode_image = images
                    .iter()
                    .find(|image| image.attr("alt") == Some("Unicode image"))
                    .unwrap();
                assert_eq!(
                    urlencoding::decode(unicode_image.attr("src").unwrap()).unwrap(),
                    "/api/attachments/٣",
                    "ordinary authored images are not rewritten as mounted attachments"
                );
                assert!(unicode_image.attr("data-public-image").is_none());
            }
            Scope::Published => {
                assert_eq!(ascii.attr("data-public-download"), Some("32"));
                assert!(ascii.attr("data-attachment").is_none());
                let images = document
                    .select(&scraper::Selector::parse("img").unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(
                    images
                        .iter()
                        .filter(|image| image.attr("data-public-image") == Some("31"))
                        .count(),
                    1,
                    "ASCII attachment image retains its established public hook"
                );
                assert!(images.iter().all(|image| image.attr("src").is_none()));
                assert!(
                    !images
                        .iter()
                        .any(|image| image.attr("data-public-image") == Some("٣")),
                    "original public source grammar rejects Unicode attachment IDs: {html}"
                );
            }
        }
    }
}

#[test]
fn native_markdown_master_private_attachment_hooks_require_ascii_digits_at_every_mount() {
    assert_attachment_identifiers_use_ascii_digits(Scope::Private);
}

#[test]
fn native_markdown_master_public_attachment_hooks_require_ascii_digits_at_every_mount() {
    assert_attachment_identifiers_use_ascii_digits(Scope::Published);
}
