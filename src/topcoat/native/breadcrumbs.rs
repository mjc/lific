//! Shared detail trails; route-specific destinations remain with their callers.

use super::{browser, deferred_delete::ToastErrorRequest, icons, navigation};
use topcoat::{
    context::Cx,
    runtime::{BoolSurrogate, Event, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(crate) const LINK_CLASS: &str = "flex min-w-0 items-center gap-1.5 text-body-sm font-medium text-[var(--text-muted)] transition-colors hover:text-[var(--text)]";
pub(crate) const LABEL_CLASS: &str = "truncate max-w-[9rem] sm:max-w-[14rem]";

pub(crate) struct Segment<'a> {
    pub content: BoxView<'a>,
    pub hide_below_sm: bool,
    pub copy: Option<String>,
}

pub(crate) fn link<'a>(cx: &'a Cx, label: &str, path: &str, mono: bool) -> BoxView<'a> {
    let label = label.to_owned();
    let class = if mono {
        format!("{LINK_CLASS} font-mono")
    } else {
        LINK_CLASS.to_owned()
    };
    let attributes = navigation::attrs(cx, path);
    view! {
        cx =>
        <a title=(label.clone()) class=(class) (attributes)>
            <span class=(LABEL_CLASS) data-label="">(label)</span>
        </a>
    }
    .boxed()
}

pub(crate) fn current<'a>(cx: &'a Cx, label: &str, mono: bool) -> BoxView<'a> {
    let label = label.to_owned();
    let class = if mono {
        "flex min-w-0 items-center gap-1.5 font-mono text-body-sm font-medium text-[var(--text)]"
    } else {
        "flex min-w-0 items-center gap-1.5 text-body-sm font-medium text-[var(--text)]"
    };
    view! {
        cx =>
        <span title=(label.clone()) aria-current="page" class=(class)>
            <span class=(LABEL_CLASS) data-label="">(label)</span>
        </span>
    }
    .boxed()
}

pub(crate) fn render<'a>(cx: &'a Cx, account_id: i64, segments: Vec<Segment<'a>>) -> BoxView<'a> {
    let mut prior_hidden = true;
    let rows = segments
        .into_iter()
        .enumerate()
        .map(|(index, segment)| {
            let separator_hidden = segment.hide_below_sm || prior_hidden;
            prior_hidden &= segment.hide_below_sm;
            (index, separator_hidden, segment)
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <nav
            aria-label="Breadcrumb"
            class="native-breadcrumbs native-issue-detail__breadcrumbs min-w-0"
        >
            <ol class="flex min-w-0 items-center gap-1.5">
                #[key(index)]
                for (index, separator_hidden, segment) in rows {
                    if index > 0 {
                        <li
                            aria-hidden="true"
                            class=(if separator_hidden {
                                "hidden shrink-0 items-center sm:flex"
                            } else {
                                "flex shrink-0 items-center"
                            })
                        >
                            <span class="text-[var(--text-faint)]">
                                (icons::ui_icon(cx, icons::UiIcon::BreadcrumbSeparator, 12))
                            </span>
                        </li>
                    }
                    <li
                        class=(if segment.hide_below_sm {
                            "group hidden min-w-0 items-center sm:flex"
                        } else {
                            "group flex min-w-0 items-center"
                        })
                    >
                        (segment.content)
                        if let Some(value) = segment.copy {
                            (copy_button(cx, account_id, index, value))
                        }
                    </li>
                }
            </ol>
        </nav>
    }
    .boxed()
}

fn copy_button(cx: &Cx, account_id: i64, index: usize, value: String) -> BoxView<'_> {
    let state_cx = cx.keyed((account_id, index));
    let copied = signal(&state_cx, || false);
    let timer = signal(&state_cx, || 0_usize);
    let completed_copied = copied.clone();
    let browser = browser::bindings();
    let label = format!("Copy {value}");
    let copy_value = value;
    let handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.prevent_default();
            event.stop_propagation();
            let _completed = |success: BoolSurrogate| {
                if success {
                    completed_copied.set(true);
                    browser.clear_timeout(timer.get());
                    let _reset = || completed_copied.set(false);
                    timer.set(browser.set_timeout(1_500_usize, _reset));
                } else {
                    let _error = ToastErrorRequest {
                        account_id,
                        message: "Couldn't copy to clipboard".to_owned(),
                    };
                    raw!(
                        "window.dispatchEvent(new CustomEvent('lific:native-toast-error',{detail:${_error},cancelable:true}));",
                        (),
                    );
                }
            };
            browser.write_clipboard(copy_value.clone(), _completed);
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    view! {
        cx =>
        <button
            type="button"
            title=(label.clone())
            aria-label=(label)
            class="hidden w-0 shrink-0 place-items-center overflow-hidden rounded text-[var(--text-faint)] opacity-0 transition-all hover:text-[var(--accent)] group-hover:w-5 group-hover:opacity-100 focus-visible:w-5 focus-visible:opacity-100 sm:grid"
            (attributes)
        >
            <span :hidden=$(copied.get())>
                (icons::ui_icon(cx, icons::UiIcon::Copy, 12))
            </span>
            <span :hidden=$(!copied.get())>
                (icons::ui_icon(cx, icons::UiIcon::Copied, 12))
            </span>
        </button>
    }.boxed()
}
