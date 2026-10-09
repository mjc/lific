//! Markdown is parsed and sanitized in Rust before entering a native view.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Node};
use topcoat::context::Cx;

use super::transport::mounted_url;

pub(crate) mod images;

static REFERENCES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?-u:\b)[A-Z][A-Z0-9]{1,4}-[0-9]+#comment-[1-9][0-9]*(?-u:\b)|",
        r"(?-u:\b)[A-Z][A-Z0-9]{1,4}-(?:(?:DOC|PLAN)-)?[0-9]+(?-u:\b)|",
        r"#[1-9][0-9]*(?-u:\b)|@[A-Za-z0-9_-]+"
    ))
    .expect("fixed Markdown reference grammar")
});

static ATTACHMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:https?://[^/]+)?/api/attachments/([0-9]+)/?$")
        .expect("fixed attachment URL grammar")
});

// The AST retains the full fence info; generated language classes only retain
// its first word. Delegate literal escaping and all code HTML to Comrak.
fn format_node<'a>(
    context: &mut comrak::html::Context<()>,
    node: comrak::nodes::Node<'a>,
    entering: bool,
) -> Result<comrak::html::ChildRendering, std::fmt::Error> {
    use std::fmt::Write as _;
    if let comrak::nodes::NodeValue::CodeBlock(ref block) = node.data().value {
        if entering {
            write!(
                context,
                "<div class=\"code-block-wrapper\" data-lang=\"{}\">",
                ammonia::clean_text(&block.info.to_lowercase())
            )?;
        }
        let children = comrak::html::format_node_default(context, node, entering)?;
        if !entering {
            context.write_str("</div>")?;
        }
        return Ok(children);
    }
    comrak::html::format_node_default(context, node, entering)
}

#[derive(Clone, Copy)]
pub(crate) enum Scope {
    Private,
    Published,
}

pub(crate) fn render_published(cx: &Cx, project: &str, source: &str) -> String {
    let html = render(cx, source, Scope::Published, &[]);
    rewrite_elements(&html, |element| match element.value().name() {
        "img" => {
            let id = element.value().attr("data-public-image")?;
            let original = published_attachment_url(cx, project, id);
            let thumbnail = format!("{original}/thumbnail");
            Some(attachment_image(element, &original, &thumbnail))
        }
        "a" => {
            let href = element.value().attr("href")?;
            let mount = super::transport::trusted_mount(cx);
            let logical = mount
                .and_then(|prefix| href.strip_prefix(prefix))
                .filter(|path| path.starts_with('/'))
                .unwrap_or(href);
            if let Some(attachment) = ATTACHMENT.captures(logical) {
                let id = attachment
                    .get(1)
                    .expect("attachment URL grammar has an ID")
                    .as_str();
                let href = published_attachment_url(cx, project, id);
                return Some(element_with_attributes(
                    element,
                    &[("href", &href)],
                    &["href", "data-public-download"],
                ));
            }
            if href.starts_with('#')
                || href.starts_with("https://")
                || href.starts_with("http://")
                || href.starts_with("mailto:")
            {
                return element
                    .value()
                    .attr("data-public-download")
                    .map(|_| element_with_attributes(element, &[], &["data-public-download"]));
            }
            let path = if logical.starts_with("/public/") {
                logical.to_owned()
            } else {
                format!("/public{logical}")
            };
            use super::public_route::Route;
            let destination = match super::public_route::resolve(&path) {
                Some(
                    Route::Issues { project }
                    | Route::Board { project }
                    | Route::IssueDetail { project, .. }
                    | Route::Pages { project }
                    | Route::PageDetail { project, .. },
                ) => project,
                _ => return Some(element.inner_html()),
            };
            if !destination.eq_ignore_ascii_case(project) {
                return Some(element.inner_html());
            }
            let href = mounted_url(cx, &path);
            Some(element_with_attributes(
                element,
                &[("href", &href)],
                &["href", "data-public-download"],
            ))
        }
        _ => None,
    })
}

fn published_attachment_url(cx: &Cx, project: &str, id: &str) -> String {
    mounted_url(
        cx,
        &format!("/public/api/projects/{project}/attachments/{id}"),
    )
}

pub(crate) fn render(cx: &Cx, source: &str, scope: Scope, mentions: &[(&str, &str)]) -> String {
    let mut options = comrak::Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.render.hardbreaks = true;
    // Raw HTML is permitted only as intermediate input to the final sanitizer.
    options.render.r#unsafe = true;
    let arena = comrak::Arena::new();
    let normalized = source.replace("\\n", "\n");
    let root = comrak::parse_document(&arena, &normalized, &options);
    let mut html = String::new();
    comrak::html::format_document_with_formatter(
        root,
        &options,
        &mut html,
        &comrak::options::Plugins::default(),
        format_node,
        (),
    )
    .expect("formatting Markdown into a String cannot fail");
    let mut document = Html::parse_fragment(&html);
    let nodes: Vec<_> = document.tree.nodes().map(|node| node.id()).collect();

    for id in nodes.into_iter().rev() {
        let node = document.tree.get(id).expect("original Markdown node");
        let replacement = match node.value() {
            Node::Text(text)
                if !node
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .any(|parent| matches!(parent.value().name(), "a" | "code" | "pre")) =>
            {
                text_references(cx, text, scope, mentions)
            }
            Node::Element(_) => {
                ElementRef::wrap(node).and_then(|element| transform_element(cx, element, scope))
            }
            _ => None,
        };
        let Some(replacement) = replacement else {
            continue;
        };

        // Let the maintained HTML parser and serializer handle fragments,
        // including entity decoding and malformed author HTML.
        let fragment = Html::parse_fragment(&replacement);
        let root = document.tree.extend_tree(fragment.tree).id();
        let children: Vec<_> = document
            .tree
            .get(root)
            .expect("inserted fragment")
            .children()
            .find(|child| child.value().is_element())
            .expect("fragment HTML root")
            .children()
            .map(|child| child.id())
            .collect();
        for child in children {
            document
                .tree
                .get_mut(id)
                .expect("original Markdown node")
                .insert_id_before(child);
        }
        document
            .tree
            .get_mut(id)
            .expect("original Markdown node")
            .detach();
    }

    sanitize(&document.root_element().inner_html(), scope)
}

/// Add trusted interactive metadata only after Markdown HTML has been sanitized.
pub(crate) fn decorate_private_images(cx: &Cx, html: &str) -> String {
    rewrite_elements(html, |element| {
        if element.value().name() != "img" {
            return None;
        }
        let src = element.value().attr("src")?;
        let (_, suffix) = src.rsplit_once("/api/attachments/")?;
        let id = suffix.strip_suffix('/').unwrap_or(suffix);
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let original = mounted_url(cx, &format!("/api/attachments/{id}"));
        if src != original {
            return None;
        }
        let thumbnail = format!("{original}/thumbnail");
        Some(attachment_image(element, &original, &thumbnail))
    })
}

fn attachment_image(element: ElementRef<'_>, original: &str, thumbnail: &str) -> String {
    element_with_attributes(
        element,
        &[
            ("src", thumbnail),
            ("loading", "lazy"),
            ("data-native-attachment-image", ""),
            ("data-native-original-src", original),
            (
                "class",
                "max-w-full max-h-[32rem] h-auto border border-solid border-[var(--border)] rounded-lg cursor-zoom-in transition-[filter] duration-150 motion-reduce:transition-none hover:brightness-95",
            ),
        ],
        &[
            "src",
            "loading",
            "data-public-image",
            "data-native-attachment-image",
            "data-native-original-src",
            "class",
        ],
    )
}

fn rewrite_elements(html: &str, transform: impl Fn(ElementRef<'_>) -> Option<String>) -> String {
    let mut document = Html::parse_fragment(html);
    let nodes: Vec<_> = document.tree.nodes().map(|node| node.id()).collect();
    for id in nodes.into_iter().rev() {
        let Some(replacement) = document
            .tree
            .get(id)
            .and_then(ElementRef::wrap)
            .and_then(&transform)
        else {
            continue;
        };
        let fragment = Html::parse_fragment(&replacement);
        let root = document.tree.extend_tree(fragment.tree).id();
        let children: Vec<_> = document
            .tree
            .get(root)
            .expect("inserted fragment")
            .children()
            .find(|child| child.value().is_element())
            .expect("fragment HTML root")
            .children()
            .map(|child| child.id())
            .collect();
        for child in children {
            document
                .tree
                .get_mut(id)
                .expect("original element")
                .insert_id_before(child);
        }
        document
            .tree
            .get_mut(id)
            .expect("original element")
            .detach();
    }
    document.root_element().inner_html()
}

fn application_url(cx: &Cx, path: &str, scope: Scope) -> String {
    let path = match scope {
        Scope::Private => path.to_owned(),
        Scope::Published => format!("/public{path}"),
    };
    mounted_url(cx, &path)
}

fn text_references(cx: &Cx, text: &str, scope: Scope, mentions: &[(&str, &str)]) -> Option<String> {
    let mut output = String::new();
    let mut consumed = 0;
    let mut changed = false;
    for reference in REFERENCES.find_iter(text) {
        let value = reference.as_str();
        let preceding = text[..reference.start()].chars().next_back();
        let replacement = if let Some(name) = value.strip_prefix('@') {
            if preceding.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '-'))
            {
                None
            } else {
                mentions.iter().find(|(visible, _)| visible.eq_ignore_ascii_case(name)).map(|(_, display)| {
                    format!(
                        "<span class=\"mention-chip\" data-mention=\"{}\" title=\"@{}\">@{}</span>",
                        ammonia::clean_text(name), ammonia::clean_text(name), ammonia::clean_text(display)
                    )
                })
            }
        } else if let Some(number) = value.strip_prefix('#') {
            if preceding.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '&' | '-'))
            {
                None
            } else {
                Some(format!(
                    "<a class=\"comment-ref\" href=\"#comment-{number}\">{value}</a>"
                ))
            }
        } else {
            let (identifier, comment) = value
                .split_once("#comment-")
                .map_or((value, None), |(ident, number)| (ident, Some(number)));
            let (project, _) = identifier
                .split_once('-')
                .expect("matched project identifier");
            let (path, issue) = if identifier.contains("-DOC-") {
                (format!("/{project}/pages"), false)
            } else if identifier.contains("-PLAN-") {
                (format!("/{project}/plans"), false)
            } else {
                (format!("/{project}/issues/{identifier}"), true)
            };
            let path =
                comment.map_or_else(|| path.clone(), |number| format!("{path}?comment={number}"));
            let data = if issue {
                format!(" data-issue-ident=\"{identifier}\"")
            } else {
                String::new()
            };
            Some(format!(
                "<a class=\"identifier-link\" href=\"{}\"{data}>{value}</a>",
                ammonia::clean_text(&application_url(cx, &path, scope))
            ))
        };
        output.push_str(&ammonia::clean_text(&text[consumed..reference.start()]));
        if let Some(replacement) = replacement {
            changed = true;
            output.push_str(&replacement);
        } else {
            output.push_str(&ammonia::clean_text(value));
        }
        consumed = reference.end();
    }
    output.push_str(&ammonia::clean_text(&text[consumed..]));
    changed.then_some(output)
}

fn transform_element(cx: &Cx, element: ElementRef<'_>, scope: Scope) -> Option<String> {
    match element.value().name() {
        "input" => {
            if element.value().attr("type") != Some("checkbox") {
                return Some(String::new());
            }
            let checked = if element.value().attr("checked").is_some() {
                " checked"
            } else {
                ""
            };
            Some(format!("<input type=\"checkbox\" disabled{checked}>"))
        }
        "img" => {
            let src = element.value().attr("src").unwrap_or("");
            // Published bodies accept only this instance's relative image paths.
            let attachment = ATTACHMENT.captures(src).filter(|_| {
                matches!(scope, Scope::Private) || src.starts_with("/api/attachments/")
            });
            let alt = ammonia::clean_text(element.value().attr("alt").unwrap_or(""));
            match (scope, attachment) {
                (Scope::Published, Some(attachment)) => Some(format!(
                    "<img alt=\"{alt}\" data-public-image=\"{}\">",
                    &attachment[1]
                )),
                (Scope::Published, None) => Some(alt),
                (Scope::Private, Some(attachment)) => {
                    let src = mounted_url(cx, &format!("/api/attachments/{}", &attachment[1]));
                    Some(element_with_attributes(
                        element,
                        &[("src", src.as_str())],
                        &["src"],
                    ))
                }
                (Scope::Private, None) => None,
            }
        }
        "a" => {
            let href = element.value().attr("href")?;
            if let Some(attachment) = ATTACHMENT.captures(href) {
                let href = mounted_url(cx, &format!("/api/attachments/{}", &attachment[1]));
                let mut attributes = vec![
                    ("href", href.as_str()),
                    ("class", "attachment-chip"),
                    ("download", ""),
                ];
                match scope {
                    Scope::Private => attributes.push(("data-attachment", "")),
                    Scope::Published => attributes.push((
                        "data-public-download",
                        attachment.get(1).expect("attachment ID").as_str(),
                    )),
                }
                return Some(element_with_attributes(
                    element,
                    &attributes,
                    &[
                        "href",
                        "class",
                        "download",
                        "data-attachment",
                        "data-public-download",
                    ],
                ));
            }
            if let Some(path) = href.strip_prefix("#/") {
                let href = application_url(cx, &format!("/{path}"), scope);
                return Some(element_with_attributes(
                    element,
                    &[("href", href.as_str())],
                    &["href"],
                ));
            }
            None
        }
        _ => None,
    }
}

fn element_with_attributes(
    element: ElementRef<'_>,
    added: &[(&str, &str)],
    replaced: &[&str],
) -> String {
    let mut html = format!("<{}", element.value().name());
    for (name, value) in element
        .value()
        .attrs()
        .filter(|(name, _)| !replaced.contains(name))
        .chain(added.iter().copied())
    {
        html.push_str(&format!(" {name}=\"{}\"", ammonia::clean_text(value)));
    }
    html.push('>');
    if element.value().name() != "img" {
        html.push_str(&element.inner_html());
        html.push_str(&format!("</{}>", element.value().name()));
    }
    html
}

fn sanitize(html: &str, scope: Scope) -> String {
    let mut sanitizer = ammonia::Builder::default();
    sanitizer
        .link_rel(None)
        .add_tags(&["input"])
        .add_tag_attributes("input", &["type", "disabled", "checked"])
        .add_tag_attributes("div", &["class", "data-lang"])
        .add_tag_attributes("code", &["class"])
        .add_tag_attributes("span", &["class", "data-mention"])
        .add_tag_attributes(
            "a",
            &["class", "data-issue-ident", "data-attachment", "download"],
        );
    if let Scope::Published = scope {
        sanitizer
            .add_tag_attributes("img", &["data-public-image"])
            .add_tag_attributes("a", &["data-public-download"])
            .rm_tags(&[
                "video", "audio", "source", "track", "picture", "iframe", "object", "embed",
                "style", "link", "form", "svg", "image", "use", "math",
            ])
            .add_clean_content_tags(&["style", "svg", "math"])
            .attribute_filter(|_, attribute, value| {
                if matches!(
                    attribute,
                    "src" | "srcset" | "style" | "poster" | "background" | "ping"
                ) {
                    None
                } else {
                    Some(value.into())
                }
            });
    }
    sanitizer.clean(html).to_string()
}

#[cfg(test)]
mod tests;
