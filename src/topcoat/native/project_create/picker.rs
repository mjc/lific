//! Native picker projection; parent Rust signals own selection/disclosure.
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, shard},
    view::{View, view},
};

#[shard("/__native_project/icon_choices")]
pub(super) async fn choices(
    cx: &Cx,
    tab: String,
    query: String,
    scroll_top: usize,
    state: (Signal<String>, Signal<bool>, Signal<String>),
) -> topcoat::Result<impl View> {
    let (selected, open, search) = state;
    // Re-resolve current cookie authority for every projection. Browser strings
    // select a presentation filter, never a user or permission scope.
    let caller = super::super::session::read(cx, super::super::context::caller(cx))?;
    super::super::session::read(cx, crate::api::require_user(&caller.identity))?;
    let items: Vec<(String, String)> = match tab.as_str() {
        "icons" => super::model::icons(&query)
            .into_iter()
            .map(|name| (format!("lucide:{name}"), name))
            .collect(),
        "emoji" => super::model::emojis(&query)
            .into_iter()
            .map(|choice| (choice.value, choice.name))
            .collect(),
        _ => return Err(topcoat::router::error::bad_request("invalid project icon tab").into()),
    };
    let window = super::model::grid_window(items.len(), scroll_top);
    let total = items.len();
    let viewport = window
        .total_height
        .saturating_add(16)
        .min(super::model::VIEWPORT_HEIGHT);
    let empty_label = if tab == "icons" {
        "No icons match"
    } else {
        "No emojis match"
    };
    let visible = items
        .into_iter()
        .skip(window.start)
        .take(window.end.min(total).saturating_sub(window.start));
    let container_style = if window.virtualized {
        format!("height:{}px;position:relative", window.total_height)
    } else {
        String::new()
    };
    let grid_style = if window.virtualized {
        format!(
            "position:absolute;left:8px;right:8px;top:{}px;display:grid;grid-template-columns:repeat(8,1fr);gap:2px",
            window.offset
        )
    } else {
        "display:grid;grid-template-columns:repeat(8,1fr);gap:2px;padding:8px".into()
    };
    Ok(view! {
        cx =>
        <div
            data-native-project-picker-results=""
            style=(container_style)
            @mount=$(|_event: Event| {
                raw!(
                    "document.getElementById('native-project-icon-scroll').style.height=${viewport}.toString()+'px';",
                    (),
                );
            })
        >
            if total == 0 {
                <p>
                    (empty_label)
                    " “"
                    (query)
                    "”"
                </p>
            }
            <div style=(grid_style)>
                #[key(value.clone())]
                for (value, label) in visible {
                    <button
                        type="button"
                        aria-label=(label.clone())
                        title=(label)
                        class="native-project-picker-choice"
                        :aria-pressed=$(selected.get() == value)
                        @click=$(|_event: Event| {
                            selected.set(value.clone());
                            open.set(false);
                            search.set("".to_owned());
                            raw!(
                                "document.getElementById('native-project-icon-trigger')?.dispatchEvent(new Event('native-project-icon-change',{bubbles:true}));",
                                (),
                            );
                            raw!(
                                "queueMicrotask(() => document.getElementById('native-project-icon-trigger')?.focus());",
                                (),
                            );
                        })
                    >
                        (super::super::icons::picker_choice_icon(
                            cx,
                            Some(&value),
                            if value == "lific:logo" { 20 } else { 18 },
                        ))
                    </button>
                }
            </div>
        </div>
    })
}
