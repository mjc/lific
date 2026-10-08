//! Shared toolbar state for native document downloads.

use topcoat::{
    context::Cx,
    runtime::{Event, StringSurrogate, expr},
    view::{Attributes, BoxView, ViewExt, view},
};

use super::{browser, icons::UiIcon, transport};

#[derive(Clone, Copy)]
pub(crate) enum DocumentKind {
    Issue,
    Page,
}

impl DocumentKind {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Issue => "/__native_issue_export/",
            Self::Page => "/__native_page_export/",
        }
    }

    fn owner_id(self, identifier: &str) -> String {
        let prefix = match self {
            Self::Issue => "native-issue-export-",
            Self::Page => "native-page-export-",
        };
        format!("{prefix}{identifier}")
    }

    fn button_class(self) -> &'static str {
        match self {
            Self::Issue => "native-issue-detail__export toolbar-pill",
            Self::Page => {
                "native-page-detail__export toolbar-pill inline-flex items-center gap-1.5 rounded-full border border-[var(--border)] bg-[var(--bg-subtle)] px-4 py-[7px] text-body-sm font-medium text-[var(--text-muted)] shadow-sm transition-colors hover:bg-[var(--surface)] hover:text-[var(--text)] disabled:cursor-not-allowed disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
            }
        }
    }

    fn error_class(self) -> &'static str {
        match self {
            Self::Issue => "native-issue-detail__export-error",
            Self::Page => {
                "native-page-detail__export-error hidden sm:inline text-[var(--tc-danger)]"
            }
        }
    }

    fn error_hook(self) -> &'static str {
        match self {
            Self::Issue => "data-native-issue-export-error",
            Self::Page => "data-native-page-export-error",
        }
    }

    fn label_class(self) -> &'static str {
        match self {
            Self::Issue => "native-issue-detail__export-label",
            Self::Page => "native-page-detail__export-label hidden sm:inline",
        }
    }
}

/// Builds the independently owned export error and native button fragments.
pub(crate) fn toolbar_fragments<'a>(
    cx: &'a Cx,
    kind: DocumentKind,
    identifier: &str,
) -> (BoxView<'a>, BoxView<'a>) {
    let exporting = topcoat::runtime::signal(cx, || false);
    let error = topcoat::runtime::signal(cx, String::new);
    let error_message = error.clone();
    let error_class = kind.error_class();
    let error_hook = kind.error_hook();
    let mut error_attributes = Attributes::with_capacity(1);
    error_attributes.insert(cx, error_hook, "");
    let error_view = view! {
        cx =>
        <span
            class=(error_class)
            :hidden=$(error_message.get().is_empty())
            (error_attributes)
        >
            $(error_message.get())
        </span>
    }
    .boxed();

    let endpoint = transport::mounted_url(cx, &format!("{}{identifier}", kind.endpoint()));
    let button_id = kind.owner_id(identifier);
    let class = kind.button_class();
    let label_class = kind.label_class();
    let browser = browser::bindings();
    let completed_error = error.clone();
    let completed_exporting = exporting.clone();
    let handler = expr!(|_event: Event| {
        if browser.is_disposed() {
            return;
        }
        if !exporting.get() {
            exporting.set(true);
            error.set("".to_owned());
            let _completed = |failure: StringSurrogate| {
                if !browser.is_disposed() {
                    completed_error.set(failure);
                    completed_exporting.set(false);
                }
            };
            browser.download(endpoint.clone(), _completed);
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    let mounted_exporting = exporting.clone();
    let label_exporting = exporting.clone();
    let button = view! {
        cx =>
        <button
            id=(button_id)
            class=(class)
            type="button"
            :aria-label=$(if exporting.get() { "Exporting" } else { "Export" })
            :disabled=$(exporting.get())
            @mount=$(|_mount: Event| mounted_exporting.set(false))
            (attributes)
        >
            (super::icons::ui_icon(cx, UiIcon::Download, 14))
            <span class=(label_class) :hidden=$(label_exporting.get())>"Export"</span>
            <span class=(label_class) :hidden=$(!label_exporting.get())>
                "Exporting..."
            </span>
        </button>
    }
    .boxed();
    (error_view, button)
}
