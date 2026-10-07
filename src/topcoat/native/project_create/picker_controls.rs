//! Parent-owned native IconPicker controls; renderer/business filters stay Rust.
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

#[shard("/__native_project/selected_icon")]
async fn selected_icon(cx: &Cx, value: String) -> topcoat::Result<impl View> {
    let caller = super::super::session::read(cx, super::super::context::caller(cx))?;
    super::super::session::read(cx, crate::api::require_user(&caller.identity))?;
    Ok(super::super::icons::project_icon(cx, Some(&value), 20))
}

pub(crate) fn picker(cx: &Cx, selected: Signal<String>) -> BoxView<'_> {
    picker_with_attributes(cx, selected, Attributes::with_capacity(0))
}
pub(crate) fn picker_with_attributes(
    cx: &Cx,
    selected: Signal<String>,
    changed: Attributes,
) -> BoxView<'_> {
    let open = signal(cx, || false);
    let tab = signal(cx, || "icons".to_owned());
    let query = signal(cx, String::new);
    let scroll_top = signal(cx, || 0_usize);
    let _target_bits = usize::BITS;
    let outside_open = open.clone();
    let outside_query = query.clone();
    let mounted = expr!(|_event: Event| {
        let _outside = |_event: Event| {
            let inside = raw!(
                "cx.hydrate(!!${_event}.target.closest('[data-native-project-picker]'))",
                false
            );
            if !inside {
                outside_open.set(false);
                outside_query.set("".to_owned());
            }
        };
        let _escape = |_event: Event| {
            let key = raw!("cx.hydrate(${_event}.key)", String::new());
            if if open.get() { key == "Escape" } else { false } {
                raw!("${_event}.preventDefault();", ());
                open.set(false);
                query.set("".to_owned());
                raw!(
                    "document.getElementById('native-project-icon-trigger')?.focus();",
                    ()
                );
            }
        };
        raw!(
            "window.addEventListener('click',${_outside},{signal:cx.abortSignal}); window.addEventListener('keydown',${_escape},{signal:cx.abortSignal});",
            ()
        );
    });
    view! {
        cx =>
        <div
            class="native-project-picker"
            data-native-project-picker=""
            @mount=(mounted)
            (changed)
        >
            <button
                id="native-project-icon-trigger"
                type="button"
                class="native-project-picker__trigger"
                aria-label="Choose icon"
                :aria-expanded=$(open.get())
                @click=$(|event: Event| {
                    event.stop_propagation();
                    open.set(!open.get());
                    query.set("".to_owned());
                    scroll_top.set(0_usize);
                    if open.get() {
                        raw!(
                            "requestAnimationFrame(() => document.getElementById('native-project-icon-search')?.focus());",
                            (),
                        );
                    }
                })
            >
                <span :hidden=$(!selected.get().is_empty())>"+"</span>
                <span :hidden=$(selected.get().is_empty())>
                    selected_icon(value: $(selected.get()))
                </span>
            </button>
            <section
                class="native-project-picker__panel"
                :hidden=$(!open.get())
                aria-label="Icon picker"
                @click=$(|event: Event| event.stop_propagation())
            >
                <div class="native-project-picker__tabs">
                    for (value, label) in [("icons", "Icons"), ("emoji", "Emoji")] {
                        <button
                            type="button"
                            :aria-pressed=$(tab.get() == value)
                            @click=$(|_event: Event| {
                                tab.set(value.to_owned());
                                scroll_top.set(0_usize);
                                raw!(
                                    "document.getElementById('native-project-icon-scroll').scrollTop=0;",
                                    (),
                                );
                            })
                        >
                            (label)
                        </button>
                    }
                </div>
                <div class="native-project-picker__search">
                    <input
                        id="native-project-icon-search"
                        aria-label="Search icons"
                        type="text"
                        :value=$(query.get())
                        :placeholder=$(if tab.get() == "icons" {
                            "Search 1,900+ icons..."
                        } else {
                            "Search emojis..."
                        })
                        @input=$(|event: Event| {
                            query.set(event.target.value);
                            scroll_top.set(0_usize);
                            raw!(
                                "document.getElementById('native-project-icon-scroll').scrollTop=0;",
                                (),
                            );
                        })
                    >
                </div>
                <div
                    id="native-project-icon-scroll"
                    class="native-project-picker__scroll"
                    @scroll=$(|_event: Event| scroll_top.set(
                            raw!(
                                "cx.hydrate({t:'usize',bits:Number(${_target_bits}.toString()),v:String(Math.floor(document.getElementById('native-project-icon-scroll').scrollTop))})",
                                0_usize,
                            ),
                        ))
                >
                    super::picker::choices(
                        tab: $(tab.get()),
                        query: $(query.get()),
                        scroll_top: $(scroll_top.get()),
                        state: (selected.clone(), open.clone(), query.clone())
                    )
                </div>
                <div
                    class="native-project-picker__clear"
                    :hidden=$(selected.get().is_empty())
                >
                    <button
                        type="button"
                        class="native-project-picker__remove"
                        @click=$(|_event: Event| {
                            selected.set("".to_owned());
                            open.set(false);
                            query.set("".to_owned());
                            raw!(
                                "document.getElementById('native-project-icon-trigger')?.dispatchEvent(new Event('native-project-icon-change',{bubbles:true}));",
                                (),
                            );
                            raw!(
                                "document.getElementById('native-project-icon-trigger')?.focus();",
                                (),
                            );
                        })
                    >
                        "Remove icon"
                    </button>
                </div>
            </section>
        </div>
    }.boxed()
}
