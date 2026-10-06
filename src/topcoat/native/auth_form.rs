//! Shared password input and visibility control for signed-out forms.
use super::icons::UiIcon;
use super::{auth_shell::INPUT, icons};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(super) fn password_field<'a>(
    cx: &'a Cx,
    id: &'static str,
    autocomplete: &'static str,
    visible: Signal<bool>,
    describedby: Option<&'static str>,
    input: Attributes,
) -> BoxView<'a> {
    view! {cx =>
        <div class="relative">
            <input id=(id) :type=$(if visible.get(){"text"}else{"password"}) autocomplete=(autocomplete) aria-describedby=(describedby)
                class=(format!("{INPUT} w-full pl-3.5 pr-11")) (input)/>
            <button type="button" tabindex="-1" :aria-pressed=$(visible.get()) :aria-label=$(if visible.get(){"Hide password"}else{"Show password"}) :title=$(if visible.get(){"Hide password"}else{"Show password"})
                class="absolute inset-y-0 right-0 flex items-center px-3 text-[var(--text-faint)] hover:text-[var(--text-muted)] transition-colors focus-visible:outline-none focus-visible:text-[var(--accent)] bg-transparent border-0"
                @click=$(|_event:Event|visible.set(!visible.get()))>
                <span :hidden=$(visible.get())>(icons::ui_icon(cx,UiIcon::ShowPassword,17))</span><span :hidden=$(!visible.get())>(icons::ui_icon(cx,UiIcon::HidePassword,17))</span>
            </button>
        </div>
    }.boxed()
}
