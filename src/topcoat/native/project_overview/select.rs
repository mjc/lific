//! Native custom Select: selection and disclosure are Rust runtime state.

use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

#[derive(Clone)]
pub(super) struct OptionRow {
    pub(super) value: Option<i64>,
    pub(super) label: String,
    pub(super) initials: String,
    pub(super) username: String,
    pub(super) admin: bool,
    pub(super) created_at: String,
}

impl OptionRow {
    pub(super) fn empty(label: &str) -> Self {
        Self {
            value: None,
            label: label.into(),
            initials: String::new(),
            username: String::new(),
            admin: false,
            created_at: String::new(),
        }
    }
}

pub(super) fn select<'a>(
    cx: &'a Cx,
    id: String,
    rows: Vec<OptionRow>,
    value: Signal<Option<i64>>,
    disabled: Signal<bool>,
) -> BoxView<'a> {
    select_scoped(cx, cx, id, rows, value, disabled)
}

pub(super) fn select_scoped<'a>(
    cx: &'a Cx,
    state_cx: &Cx,
    id: String,
    rows: Vec<OptionRow>,
    value: Signal<Option<i64>>,
    disabled: Signal<bool>,
) -> BoxView<'a> {
    // A refreshed value without a matching option starts at the first row.
    let initial = rows
        .iter()
        .position(|row| row.value == value.get_untracked())
        .unwrap_or(0);
    let controls_cx = state_cx.keyed(id.as_str());
    let selected = signal(&controls_cx, || initial);
    let open = signal(&controls_cx, || false);
    let values = rows.iter().map(|row| row.value).collect::<Vec<_>>();
    let labels = rows.iter().map(|row| row.label.clone()).collect::<Vec<_>>();
    let marks = rows
        .iter()
        .map(|row| row.initials.clone())
        .collect::<Vec<_>>();
    let trigger = format!("{id}-trigger");
    let menu = format!("{id}-menu");
    let root = id.to_owned();
    let dispatch_root = id.clone();
    let outside_open = open.clone();
    let scroll_open = open.clone();
    let mounted = expr!(|_event: Event| {
        let _outside = |_event: Event| {
            let inside = raw!(
                "cx.hydrate(document.getElementById(${root}.toString())?.contains(${_event}.target) ?? false)",
                false
            );
            if !inside {
                outside_open.set(false);
            }
        };
        let _scroll = |_event: Event| {
            let internal = raw!(
                "cx.hydrate(${_event}.type === 'scroll' && (document.getElementById(${menu}.toString())?.contains(${_event}.target) ?? false))",
                false
            );
            if !internal {
                scroll_open.set(false);
            }
        };
        raw!(
            "window.addEventListener('click',${_outside},{signal:cx.abortSignal}); window.addEventListener('scroll',${_scroll},{capture:true,signal:cx.abortSignal}); window.addEventListener('resize',${_scroll},{signal:cx.abortSignal});",
            ()
        );
    });
    let position_trigger = trigger.clone();
    let position_menu = menu;
    let interaction = expr!(|event: Event| {
        let _position = || {
            // DOM measurements only; flip/clamp decisions below are Rust.
            let measurements = raw!(
                r#"cx.hydrate((() => {
            const t=document.getElementById(${position_trigger}.toString()).getBoundingClientRect();
            const menu=document.getElementById(${position_menu}.toString());
            menu.style.minWidth=t.width+'px';
            const m=menu.getBoundingClientRect();
            return [t.top,t.bottom,t.left,t.width,m.height,m.width,window.innerHeight,window.innerWidth];
        })())"#,
                (
                    0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64
                )
            );
            let trigger_top = measurements.0;
            let trigger_bottom = measurements.1;
            let trigger_left = measurements.2;
            let trigger_width = measurements.3;
            let menu_height = measurements.4;
            let menu_width = measurements.5;
            let viewport_height = measurements.6;
            let viewport_width = measurements.7;
            let below = trigger_bottom + 4.0_f64;
            let above = trigger_top - menu_height - 4.0_f64;
            let bottom_limit = viewport_height - 8.0_f64;
            let _top = if below + menu_height > bottom_limit {
                if above >= 8.0_f64 { above } else { below }
            } else {
                below
            };
            let right_limit = viewport_width - menu_width - 8.0_f64;
            let bounded_left = if trigger_left > right_limit {
                right_limit
            } else {
                trigger_left
            };
            let _left = if bounded_left < 8.0_f64 {
                8.0_f64
            } else {
                bounded_left
            };
            let _width = trigger_width;
            raw!(
                "const m=document.getElementById(${position_menu}.toString()); m.style.top=${_top}.toString()+'px';m.style.left=${_left}.toString()+'px';m.style.minWidth=${_width}.toString()+'px';",
                ()
            );
        };
        let kind = event.event_type.to_owned();
        if kind == "click" {
            event.stop_propagation();
            let opening = !open.get();
            if opening { raw!("${_position}();", ()); }
            open.set(opening);
            if opening {
                raw!("requestAnimationFrame(() => ${_position}());", ());
            }
        } else {
            if !open.get() {
                if if event.key == "Enter" {
                    true
                } else if event.key == " " {
                    true
                } else {
                    event.key == "ArrowDown"
                } {
                    event.prevent_default();
                    raw!("${_position}();", ());
                    open.set(true);
                    raw!("requestAnimationFrame(() => ${_position}());", ());
                }
            } else {
                if if event.key == "Escape" {
                    true
                } else {
                    event.key == "Enter"
                } {
                    event.prevent_default();
                    open.set(false);
                    raw!(
                        "document.getElementById(${trigger}.toString())?.focus();",
                        ()
                    );
                } else {
                    let current = selected.get();
                    let next = if event.key == "ArrowDown" {
                        event.prevent_default();
                        if current + 1_usize < values.len() {
                            current + 1_usize
                        } else {
                            current
                        }
                    } else if event.key == "ArrowUp" {
                        event.prevent_default();
                        if current > 0_usize {
                            current - 1_usize
                        } else {
                            current
                        }
                    } else {
                        current
                    };
                    selected.set(next);
                    value.set(values.index(next).clone());
                    if next != current {
                        raw!(
                            "document.getElementById(${dispatch_root}.toString())?.dispatchEvent(new Event('native-overview-selection',{bubbles:true}));",
                            ()
                        );
                    }
                }
            };
        };
    }).into_evaluated_and_js().1;
    let mut trigger_interaction = Attributes::with_capacity(2);
    trigger_interaction.insert(cx, "data-topcoat-on:click", interaction.clone());
    trigger_interaction.insert(cx, "data-topcoat-on:keydown", interaction.clone());
    let mut menu_interaction = Attributes::with_capacity(1);
    menu_interaction.insert(cx, "data-topcoat-on:keydown", interaction);
    view! { cx =>
        <div id=(id.clone()) class="native-project-select" @mount=(mounted)>
            <button id=(format!("{id}-trigger")) type="button" :disabled=$(disabled.get()) class="native-project-select__trigger"
                aria-haspopup="listbox" :aria-expanded=$(open.get()) aria-controls=(format!("{id}-menu"))
                (trigger_interaction)>
                <span class="native-project-select__selected">
                    <span class="native-project-select__avatar" :hidden=$(marks.index(selected.get()).is_empty())>$(marks.index(selected.get()).to_owned())</span>
                    <span>$(labels.index(selected.get()).to_owned())</span>
                </span>
                <span class="native-project-select__chevron" aria-hidden="true">(super::super::icons::project_icon(cx,Some("lucide:ChevronDown"),12))</span>
            </button>
            <div id=(format!("{id}-menu")) class="native-project-select__options" role="listbox" :hidden=$(!open.get())
                @click=$(|event: Event| event.stop_propagation()) (menu_interaction)>
                for (index, row) in rows.into_iter().enumerate() {
                    if row.value.is_some() {<button type="button" role="option" :aria-selected=$(selected.get() == index)
                        @click=$(|_event: Event| {
                            selected.set(index); value.set(values.index(index).clone()); open.set(false);
                            raw!("document.getElementById(${dispatch_root}.toString())?.dispatchEvent(new Event('native-overview-selection',{bubbles:true}));",());
                            raw!("document.getElementById(${trigger}.toString())?.focus();", ());
                        })>
                        <span class="native-overview__select-option"><span>(row.label)</span>if !row.username.is_empty(){<span class="native-overview__select-username">(format!("@{}",row.username))</span>}</span>
                    </button>}
                }
            </div>
        </div>
    }.boxed()
}
