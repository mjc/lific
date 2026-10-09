//! Document-owned Home scheduling. Rust owns decisions; raw code uses browser APIs.

use topcoat::{
    context::Cx,
    runtime::{Event, Js, Signal, Surrogated, expr, signal},
    view::Attributes,
};

pub(super) fn mount(cx: &Cx, inputs: Signal<String>, revision: Signal<usize>) -> Attributes {
    let content_path = super::transport::mounted_url(cx, "/__native_home/content");
    let ready = signal(cx, || false);
    let refreshing = signal(cx, || false);
    let pending = signal(cx, || false);
    let maximum_scheduled = signal(cx, || false);
    let retry_scheduled = signal(cx, || false);
    let disposed = signal(cx, || false);
    let handles = (
        &inputs,
        &revision,
        &ready,
        &refreshing,
        &pending,
        &maximum_scheduled,
        &retry_scheduled,
        &disposed,
    )
        .into_surrogate();
    let arguments = Js::builder()
        .source("[")
        .surrogate(&handles)
        .source(",")
        .surrogate(&content_path)
        .source("]")
        .build();
    let key = format!("{}#home-refresh", super::home_shell::handler_url());
    super::handler_asset::mount(cx, &key, arguments)
}

type RefreshSignals<'a> = (
    &'a Signal<String>,
    &'a Signal<usize>,
    &'a Signal<bool>,
    &'a Signal<bool>,
    &'a Signal<bool>,
    &'a Signal<bool>,
    &'a Signal<bool>,
    &'a Signal<bool>,
);

/// Timers and callbacks remain owned by the actual Home section and its signals.
pub(crate) fn handler_factory() -> Js {
    let handler = expr!(
        |_event: Event, handles: RefreshSignals<'_>, content_path: &String| {
            let inputs = handles.0;
            let revision = handles.1;
            let ready = handles.2;
            let refreshing = handles.3;
            let pending = handles.4;
            let maximum_scheduled = handles.5;
            let retry_scheduled = handles.6;
            let disposed = handles.7;
            let mount_maximum = maximum_scheduled;
            let mount_retry = retry_scheduled;
            let quiet_maximum = maximum_scheduled;
            let quiet_disposed = disposed;
            let retry_pending = pending;
            let retry_scheduled_callback = retry_scheduled;
            let run_inputs = inputs;
            let run_disposed = disposed;
            let run_ready = ready;
            let run_refreshing = refreshing;
            let run_pending = pending;
            let run_retry_scheduled = retry_scheduled;
            let schedule_disposed = disposed;
            let schedule_maximum = maximum_scheduled;
            let dispose_pending = pending;
            let mount_ready = ready;
            let mount_refreshing = refreshing;
            let mount_pending = pending;
            let mount_disposed = disposed;
            let _read = || {
                raw!(
                    r#"cx.hydrate((() => {
                        const now = new Date();
                        let storedValue = null;
                        try { storedValue = localStorage.getItem('lific_recents'); } catch {}
                        return JSON.stringify({epochMilliseconds: now.getTime(),
                            timezoneOffsetMinutes: now.getTimezoneOffset(),
                            locale: new Intl.Collator().resolvedOptions().locale, storedValue});
                    })())"#,
                    String::new()
                )
            };
            let _quiet = || {
                if !quiet_disposed.get() {
                    raw!(
                        "clearTimeout(owner.nativeHomeEager); clearTimeout(owner.nativeHomeMaximum);",
                        ()
                    );
                    quiet_maximum.set(false);
                    raw!("owner.nativeHomeRun();", ());
                };
            };
            let _retry = || {
                retry_scheduled_callback.set(false);
                if retry_pending.get() {
                    raw!("owner.nativeHomeRun();", ());
                };
            };
            let _run = || {
                let _hidden = raw!("cx.hydrate(document.hidden)", false);
                if !run_disposed.get() {
                    if !run_ready.get() {
                        run_pending.set(true);
                        if !run_retry_scheduled.get() {
                            run_retry_scheduled.set(true);
                            let _retry_delay = 2000.0;
                            raw!(
                                "owner.nativeHomeRetry=setTimeout(()=>${_retry}(),Number(${_retry_delay}.toString()));",
                                ()
                            );
                        };
                    } else {
                        if run_refreshing.get() {
                            run_pending.set(true);
                        } else {
                            if !_hidden {
                                run_pending.set(false);
                                run_refreshing.set(true);
                                let fresh_inputs = raw!("${_read}()", String::new());
                                run_inputs.set(fresh_inputs);
                                revision.increment();
                            };
                        };
                    };
                };
            };
            let _schedule = |_delay: f64, _maximum: bool| {
                let _hidden = raw!("cx.hydrate(document.hidden)", false);
                if !schedule_disposed.get() {
                    if !_hidden {
                        raw!(
                            "clearTimeout(owner.nativeHomeEager); owner.nativeHomeEager=setTimeout(()=>${_quiet}(),Number(${_delay}.toString()));",
                            ()
                        );
                        if _maximum {
                            if !schedule_maximum.get() {
                                schedule_maximum.set(true);
                                let _maximum_delay = 5000.0;
                                raw!(
                                    "owner.nativeHomeMaximum=setTimeout(()=>${_quiet}(),Number(${_maximum_delay}.toString()));",
                                    ()
                                );
                            };
                        };
                    };
                };
            };
            let _realtime = || {
                let _delay = 750.0;
                let _maximum = true;
                raw!("owner.nativeHomeSchedule(${_delay},${_maximum});", ());
            };
            let _focus = |_event: Event| {
                let _delay = 50.0;
                let _maximum = false;
                raw!("owner.nativeHomeSchedule(${_delay},${_maximum});", ());
            };
            let _visible = |_event: Event| {
                let _hidden = raw!("cx.hydrate(document.hidden)", false);
                if !_hidden {
                    let _delay = 50.0;
                    let _maximum = false;
                    raw!("owner.nativeHomeSchedule(${_delay},${_maximum});", ());
                };
            };
            let _finished = || {
                ready.set(true);
                refreshing.set(false);
                if pending.get() {
                    pending.set(false);
                    raw!("${_run}();", ());
                };
            };
            let _failed = |path: String| {
                if path == content_path {
                    raw!("${_finished}();", ());
                };
            };
            let _dispose = || {
                disposed.set(true);
                dispose_pending.set(false);
                raw!(
                    "clearTimeout(owner.nativeHomeEager); clearTimeout(owner.nativeHomeMaximum); clearTimeout(owner.nativeHomeRetry); delete owner.nativeHomeSchedule; delete owner.nativeHomeRealtime; delete owner.nativeHomeRun; delete owner.nativeHomeFinished;",
                    ()
                );
            };
            raw!(
                "const owner=${_event}.inner.target; owner.nativeHomeSchedule=${_schedule}; owner.nativeHomeRealtime=${_realtime}; owner.nativeHomeRun=${_run}; owner.nativeHomeFinished=${_finished};",
                ()
            );
            raw!(
                "owner.addEventListener('topcoat:render-error',event=>${_failed}(cx.hydrate(event.detail.path)),{signal:cx.abortSignal}); window.addEventListener('focus',event=>${_focus}(cx.event(event)),{signal:cx.abortSignal}); document.addEventListener('visibilitychange',event=>${_visible}(cx.event(event)),{signal:cx.abortSignal}); cx.abortSignal.addEventListener('abort',()=>${_dispose}(),{once:true});",
                ()
            );
            // A parent render preserves matching signals, but retires their timer
            // handles. The generic mount runs once for each new owning scope.
            mount_ready.set(false);
            mount_refreshing.set(false);
            mount_pending.set(false);
            mount_maximum.set(false);
            mount_retry.set(false);
            mount_disposed.set(false);
            let initial_inputs = raw!("${_read}()", String::new());
            inputs.set(initial_inputs);
        }
    );
    handler.into_evaluated_and_js().1
}

/// Invokes a callback authored above; the mount span carries no application state.
pub(super) fn callback(cx: &Cx, callback: &str) -> Attributes {
    let callback = callback.to_owned();
    let handler = expr!(|_event: Event| {
        raw!(
            "${_event}.inner.target.closest('[data-native-home]')[${callback}.toString()]();",
            ()
        );
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attributes
}
