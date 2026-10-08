//! Page detail presentation cues mirrored from Main.
use topcoat::{
    context::Cx,
    runtime::Signal,
    view::{BoxView, ViewExt, view},
};

pub(super) fn read_only_badge<'a>(cx: &'a Cx, workspace_page: bool) -> BoxView<'a> {
    let title = if workspace_page {
        "Read-only — workspace pages can only be edited by an admin."
    } else {
        "Read-only — you're a viewer on this project. You can still comment."
    };
    view! {
        cx =>
        <span
            class="text-micro font-medium px-1.5 py-0.5 rounded-full text-[var(--text-muted)] bg-[var(--bg-subtle)]"
            data-native-page-readonly=""
            title=(title)
        >
            "Read-only"
        </span>
    }
    .boxed()
}

pub(super) fn save_feedback<'a>(
    cx: &'a Cx,
    saving: Signal<bool>,
    last_saved: Signal<String>,
) -> BoxView<'a> {
    view! {
        cx =>
        <span
            class="hidden sm:inline text-caption text-[var(--text-faint)] sm:min-w-[5rem] text-right"
        >
            <span
                class="animate-pulse"
                data-native-page-save-feedback="saving"
                :hidden=$(!saving.get())
                role="status"
            >
                "Saving..."
            </span>
            <span
                data-native-page-save-feedback="saved"
                :hidden=$(if saving.get() { true } else { last_saved.get().is_empty() })
                :data-saved-at=$(last_saved.get())
                role="status"
            >
                "Saved at "
                $(last_saved.get())
            </span>
        </span>
    }
    .boxed()
}
