use super::*;
use regex::Regex;
use std::sync::LazyLock;

// Keep Main's deliberately prose-oriented hover excerpt, including omission of
// fenced code/images. Full touch descriptions use the shared Markdown parser.
static EXCERPT: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"```[\s\S]*?```", " "),
        (r"`([^`]*)`", "$1"),
        (r"!\[[^\]]*\]\([^)]*\)", " "),
        (r"\[([^\]]*)\]\([^)]*\)", "$1"),
        (r"(?m)^#{1,6}[\s\u{feff}]+", ""),
        (
            r"(?m)^[\s\u{feff}]*(?:[-*+]|[0-9]+\.)[\s\u{feff}]+(?:\[[ xX]\][\s\u{feff}]*)?",
            "",
        ),
        (r"(?m)^[\s\u{feff}]*>[\s\u{feff}]?", ""),
        (r"\*\*|__|\*|_|~~", ""),
        (r"[\s\u{feff}]+", " "),
    ]
    .into_iter()
    .map(|(pattern, replacement)| {
        (
            Regex::new(pattern).expect("fixed Main hover excerpt grammar"),
            replacement,
        )
    })
    .collect()
});
pub(super) fn strip(source: &str) -> String {
    let mut text = source.to_owned();
    for (pattern, replacement) in EXCERPT.iter() {
        text = pattern.replace_all(&text, *replacement).into_owned();
    }
    text.trim().to_owned()
}

pub(super) fn hover<'a>(
    cx: &'a Cx,
    identifier: &str,
    data: Result<data::Data, crate::error::LificError>,
) -> BoxView<'a> {
    let content = match data {
        Ok(data) => {
            let issue = data.issue;
            let description = strip(&issue.description);
            let module = data
                .modules
                .iter()
                .find(|module| Some(module.id) == issue.module_id);
            let module_view = module.map(|module| {
                let icon = module_icon(cx, module.emoji.as_deref());
                let name = module.name.clone();
                view! {
                    cx =>
                    <div
                        class="mt-1.5 flex items-center gap-1 text-micro text-[var(--text-muted)]"
                    >
                        (icon)
                        (name)
                    </div>
                }
                .boxed()
            });
            view! {
                cx =>
                <div class="flex items-center gap-1.5">
                    (icons::status_icon(cx, issue.status, 14))
                    <span class="font-mono text-micro text-[var(--text-faint)]">
                        (issue.identifier)
                    </span>
                    if issue.priority != crate::db::models::Priority::None {
                        <span class="ml-auto shrink-0">
                            (icons::priority_icon(cx, issue.priority, 13))
                        </span>
                    }
                </div>
                <p
                    class="text-body-sm text-[var(--text)] leading-snug line-clamp-2 mt-1 mb-0"
                >
                    (issue.title)
                </p>
                if description.is_empty() {
                    <p
                        class="text-caption text-[var(--text-faint)] italic mt-1.5 mb-0 pt-1.5 border-t border-[var(--border)]"
                    >
                        "No description"
                    </p>
                } else {
                    <p
                        class="text-caption text-[var(--text-muted)] leading-snug line-clamp-5 mt-1.5 mb-0 pt-1.5 border-t border-[var(--border)]"
                    >
                        (description)
                    </p>
                }
                if let Some(module) = module_view {
                    (module)
                }
            }.boxed()
        }
        Err(_) => {
            let message = format!("{identifier} isn't available");
            view! {
                cx =>
                <p class="text-body-sm text-[var(--text-faint)] italic m-0">
                    (message)
                </p>
            }
            .boxed()
        }
    };
    view! {
        cx =>
        <div
            role="tooltip"
            class="z-[1000] w-[270px] rounded-lg border border-[var(--border)] bg-[var(--surface)] shadow-[0_8px_24px_rgba(0,0,0,0.22)] px-3 py-2.5 text-left transition-opacity duration-100"
        >
            (content)
        </div>
    }.boxed()
}

fn module_icon<'a>(cx: &'a Cx, value: Option<&str>) -> BoxView<'a> {
    match value {
        Some(value) => icons::project_icon(cx, Some(value), 11),
        None => icons::ui_icon(cx, icons::UiIcon::Modules, 10),
    }
}
