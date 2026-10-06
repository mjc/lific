//! One subordinate handle bundle; scalar loading updates preserve row nodes.
use super::{
    model,
    recents_actions::{self, Published, failed, finish, prepare, read},
    recents_model::State,
};
use topcoat::{
    context::Cx,
    runtime::{
        BoolSurrogate, Event, I64Surrogate, Js, Signal, SignalSurrogate, StringSurrogate,
        Surrogated, expr, signal,
    },
    view::Attributes,
};

#[derive(Clone)]
pub(super) struct Signals {
    pub(super) account: i64,
    pub(super) cache: Signal<String>,
    pub(super) rows: Signal<String>,
    pub(super) label: Signal<String>,
    pub(super) project: Signal<i64>,
    pub(super) visible: Signal<bool>,
    pub(super) loading: Signal<bool>,
    pub(super) error: Signal<String>,
    pub(super) open: Signal<bool>,
    pub(super) focus: Signal<String>,
    pub(super) status: Signal<String>,
    pub(super) entered: Signal<String>,
    pub(super) request: Signal<i64>,
}
pub(super) type Handles = (
    Signal<String>,
    Signal<String>,
    Signal<String>,
    Signal<i64>,
    Signal<bool>,
    Signal<bool>,
    Signal<String>,
    Signal<bool>,
    Signal<String>,
    Signal<String>,
    Signal<String>,
    Signal<i64>,
);
impl Signals {
    pub(super) fn new(
        cx: &Cx,
        account: i64,
        catalog: &model::State,
        path: &str,
    ) -> Result<Self, crate::error::LificError> {
        let owner = cx.keyed(format!("native-sidebar-recents-owner-{account}"));
        let mut state = State::new(account, false);
        let project = recents_actions::selected(catalog, path);
        let section = project
            .and_then(|project| super::recents_model::Section::for_path(&project.identifier, path));
        if let Some(token) = state.begin(account, project, section, true) {
            let caller = super::super::context::caller(cx)?;
            let rows = crate::services::project_recents::load(
                super::super::context::db(cx),
                &caller.identity,
                account,
                token.project.0,
                token.section,
            );
            state.finish(token, rows, None);
        }
        let value = recents_actions::published(&state, String::new())?;
        Ok(Self {
            account,
            cache: signal(&owner, || value.0),
            rows: signal(&owner, || value.1),
            label: signal(&owner, || value.2),
            project: signal(&owner, || value.3),
            visible: signal(&owner, || value.4),
            loading: signal(&owner, || value.5),
            error: signal(&owner, || value.6),
            open: signal(&owner, || value.7),
            focus: signal(&owner, || value.8),
            status: signal(&owner, || value.9),
            entered: signal(&owner, || path.to_owned()),
            request: signal(&owner, || 0_i64),
        })
    }
    pub(super) fn handles(&self) -> Handles {
        (
            self.cache.clone(),
            self.rows.clone(),
            self.label.clone(),
            self.project.clone(),
            self.visible.clone(),
            self.loading.clone(),
            self.error.clone(),
            self.open.clone(),
            self.focus.clone(),
            self.status.clone(),
            self.entered.clone(),
            self.request.clone(),
        )
    }
    pub(super) fn from_handles(account: i64, h: Handles) -> Self {
        Self {
            account,
            cache: h.0,
            rows: h.1,
            label: h.2,
            project: h.3,
            visible: h.4,
            loading: h.5,
            error: h.6,
            open: h.7,
            focus: h.8,
            status: h.9,
            entered: h.10,
            request: h.11,
        }
    }
}

/// Called by the single route driver, or the actual refresh control. Rust owns
/// state decisions; raw adapters only inspect DOM focus and await native calls.
pub(super) fn refresh(
    cx: &Cx,
    state: &Signals,
    catalog: Signal<String>,
    path: String,
    event_name: &str,
    force: bool,
) -> Attributes {
    let arguments = Js::builder()
        .raw("[")
        .surrogate(&(
            (&state.cache).into_surrogate(),
            (&state.rows).into_surrogate(),
            (&state.label).into_surrogate(),
            (&state.project).into_surrogate(),
            (&state.visible).into_surrogate(),
            (&state.loading).into_surrogate(),
            (&state.error).into_surrogate(),
            (&state.open).into_surrogate(),
            (&state.focus).into_surrogate(),
            (&state.status).into_surrogate(),
            (&state.entered).into_surrogate(),
            (&state.request).into_surrogate(),
        ))
        .raw(",")
        .surrogate(&(&catalog).into_surrogate())
        .raw(",")
        .surrogate(&state.account.into_surrogate())
        .raw(",")
        .surrogate(&(&path).into_surrogate())
        .raw(",")
        .surrogate(&force.into_surrogate())
        .raw("]")
        .build();
    let key = format!("{}#recents-refresh", super::handler_url());
    super::super::handler_asset::event(cx, &key, arguments, event_name)
}

type RecentsHandlerSignals<'a> = (
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<i64>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<i64>,
);

/// Shared Rust browser code receives the subordinate owner on every invocation.
pub(crate) fn handler_factory() -> Js {
    let handler = expr!(|event: Event,
                         handles: RecentsHandlerSignals<'_>,
                         catalog: &SignalSurrogate<String>,
                         account: I64Surrogate,
                         path: &StringSurrogate,
                         force: BoolSurrogate| {
        let cache = handles.0;
        let rows = handles.1;
        let label = handles.2;
        let project = handles.3;
        let visible = handles.4;
        let loading = handles.5;
        let error = handles.6;
        let open = handles.7;
        let focus = handles.8;
        let status = handles.9;
        let entered = handles.10;
        let request = handles.11;
        let transport_loading = loading;
        let transport_status = status;
        let transport_error = error;
        let transport_request = request;
        let run_cache = cache;
        let failed_cache = cache;
        let failed_request = request;

        if force {
            event.prevent_default();
        }
        let refresh_needed = if force { true } else { entered.get() != path };
        if refresh_needed {
            if request.get() >= 0_i64 {
                if request.get() < 9_007_199_254_740_990_i64 {
                    let ticket = request.get() + 1_i64;
                    request.set(ticket);
                    entered.set(path.clone());
                    let _publish = |value: <Published as topcoat::runtime::Surrogated>::Surrogate, expected: <String as topcoat::runtime::Surrogated>::Surrogate| {
                let current = raw!(
                    "cx.hydrate(document.activeElement?.closest('[data-recents-list] a')?.getAttribute('data-recents-href')??'')",
                    String::new()
                );
                let unchanged = if expected.is_empty() { false } else { current == expected };
                cache.set(value.0);
                if rows.get() != value.1 {
                    rows.set(value.1);
                }
                label.set(value.2);
                project.set(value.3);
                visible.set(value.4);
                loading.set(value.5);
                error.set(value.6);
                if unchanged {
                    focus.set(value.8);
                } else {
                    focus.set("".to_owned());
                }
                status.set(value.9);
            };
                    let _transport = || {
                        let alive = raw!("cx.hydrate(!cx.abortSignal.aborted)", false);
                        if alive {
                            if ticket == transport_request.get() {
                                transport_loading.set(false);
                                transport_status.set("".to_owned());
                                transport_error
                                    .set("Could not load recent items. Try again.".to_owned());
                            }
                        }
                    };
                    let _run = async || {
                        let prepared = prepare(
                            account,
                            run_cache.get(),
                            catalog.get(),
                            path.clone(),
                            open.get(),
                        )
                        .await;
                        let alive = raw!("cx.hydrate(!cx.abortSignal.aborted)", false);
                        if alive {
                            if ticket == request.get() {
                                let _prepared_value = prepared.0;
                                raw!("${_publish}(${_prepared_value}, cx.hydrate(''));", ());
                                if !prepared.1.is_empty() {
                                    let failed_token = prepared.1.clone();
                                    let _load = async || {
                                        let result = read(account, prepared.1.clone()).await;
                                        let _focused = raw!(
                                            "cx.hydrate(document.activeElement?.closest('[data-recents-list] a')?.getAttribute('data-recents-href')??'')",
                                            String::new()
                                        );
                                        let merged = finish(
                                            account,
                                            run_cache.get(),
                                            prepared.1.clone(),
                                            result,
                                            _focused.clone(),
                                        )
                                        .await;
                                        let alive =
                                            raw!("cx.hydrate(!cx.abortSignal.aborted)", false);
                                        if alive {
                                            if ticket == request.get() {
                                                let _merged_value = merged;
                                                raw!(
                                                    "${_publish}(${_merged_value}, ${_focused});",
                                                    ()
                                                );
                                            }
                                        }
                                    };
                                    let _failed = async || {
                                        let _focused = raw!(
                                            "cx.hydrate(document.activeElement?.closest('[data-recents-list] a')?.getAttribute('data-recents-href')??'')",
                                            String::new()
                                        );
                                        let merged = failed(
                                            account,
                                            failed_cache.get(),
                                            failed_token.clone(),
                                            _focused.clone(),
                                        )
                                        .await;
                                        let alive =
                                            raw!("cx.hydrate(!cx.abortSignal.aborted)", false);
                                        if alive {
                                            if ticket == failed_request.get() {
                                                let _merged_value = merged;
                                                raw!(
                                                    "${_publish}(${_merged_value}, ${_focused});",
                                                    ()
                                                );
                                            }
                                        }
                                    };
                                    raw!(
                                        "Promise.resolve().then(()=>${_load}()).catch(()=>${_failed}()).catch(()=>${_transport}());",
                                        ()
                                    );
                                }
                            }
                        }
                    };
                    raw!(
                        "Promise.resolve().then(()=>${_run}()).catch(()=>${_transport}());",
                        ()
                    );
                }
            }
        }
    });
    handler.into_evaluated_and_js().1
}

pub(super) fn disclosure(cx: &Cx, state: &Signals) -> Attributes {
    let open = state.open.clone();
    let toggle = expr!(|_event: Event| {
        open.set(!open.get());
        let _value = if open.get() { "1" } else { "0" };
        raw!(
            "try{sessionStorage.setItem('lific:sidebar:recents-open',${_value}.toString());}catch{}",
            ()
        );
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        toggle.into_evaluated_and_js().1,
    );
    attributes
}
pub(super) fn storage(cx: &Cx, state: &Signals) -> Attributes {
    let open = state.open.clone();
    let mounted = expr!(|_event: Event| {
        let value = raw!(
            "cx.hydrate((()=>{try{return sessionStorage.getItem('lific:sidebar:recents-open')??'';}catch{return '';}})())",
            String::new()
        );
        open.set(value == "1");
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        mounted.into_evaluated_and_js().1,
    );
    attributes
}
