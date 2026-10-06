//! One document clock, browser timezone formatting, and Rust relative decisions.
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, View, ViewExt, component, view},
};

pub(crate) fn clock_mount(cx: &Cx, now: Signal<f64>) -> Attributes {
    let handler = expr!(|_event: Event| {
        raw!("let interval;", ());
        let _tick = || now.set(raw!("cx.hydrate(Date.now())", 0.0));
        let _visible = |_event: Event| {
            let hidden = raw!("cx.hydrate(document.visibilityState === 'hidden')", false);
            if hidden {
                raw!("clearInterval(interval); interval=undefined;", ());
            } else {
                raw!(
                    "${_tick}(); if(interval===undefined) interval=setInterval(()=>${_tick}(),30000);",
                    ()
                );
            }
        };
        raw!(
            "${_visible}(${_event}); document.addEventListener('visibilitychange',event=>${_visible}(cx.event(event)),{signal:cx.abortSignal}); cx.abortSignal.addEventListener('abort',()=>clearInterval(interval),{once:true});",
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

pub(crate) fn absolute<'a>(cx: &'a Cx, timestamp: &str) -> BoxView<'a> {
    let datetime = timestamp.to_owned();
    let timestamp = timestamp.to_owned();
    let text = signal(cx, || timestamp.clone());
    view! { cx => <time datetime=(datetime) @mount=$(|_event: Event| {
        text.set(raw!("cx.hydrate(new Date(${timestamp}.toString()+'Z').toLocaleDateString('en-US',{month:'short',day:'numeric',year:'numeric',hour:'numeric',minute:'2-digit'}))", String::new()));
    })>$(text.get())</time> }.boxed()
}

pub(crate) fn relative<'a>(
    cx: &'a Cx,
    timestamp: &str,
    now: Signal<f64>,
) -> (BoxView<'a>, Signal<String>) {
    let date = chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S%.f")
        .map_or(f64::NAN, |date| date.and_utc().timestamp_millis() as f64);
    let datetime = timestamp.to_owned();
    let timestamp = timestamp.to_owned();
    let full = signal(cx, || timestamp.clone());
    let fallback = signal(cx, || timestamp.clone());
    let local_full = full.clone();
    let time = view! { cx => <time datetime=(datetime) :title=$(full.get()) @mount=$(|_event: Event| {
        full.set(raw!("cx.hydrate(new Date(${timestamp}.toString()+'Z').toLocaleDateString('en-US',{month:'short',day:'numeric',year:'numeric',hour:'numeric',minute:'2-digit'}))", String::new()));
        fallback.set(raw!("cx.hydrate(new Date(${timestamp}.toString()+'Z').toLocaleDateString('en-US',{month:'short',day:'numeric'}))", String::new()));
    })>
    $(if (now.get() - date) < 60000.0 { "just now".to_owned() }
        else { if (now.get() - date) < 3600000.0 { let epoch = now.get(); let minutes = raw!("cx.hydrate(Math.floor((Number(${epoch}.toString())-Number(${date}.toString()))/60000))", ((epoch-date)/60000.0).floor()); raw!("cx.hydrate(${minutes}.toString()+'m ago')",format!("{minutes}m ago")) }
        else { if (now.get() - date) < 86400000.0 { let epoch = now.get(); let hours = raw!("cx.hydrate(Math.floor((Number(${epoch}.toString())-Number(${date}.toString()))/3600000))", ((epoch-date)/3600000.0).floor()); raw!("cx.hydrate(${hours}.toString()+'h ago')",format!("{hours}h ago")) }
        else { if (now.get() - date) < 604800000.0 { let epoch = now.get(); let days = raw!("cx.hydrate(Math.floor((Number(${epoch}.toString())-Number(${date}.toString()))/86400000))", ((epoch-date)/86400000.0).floor()); raw!("cx.hydrate(${days}.toString()+'d ago')",format!("{days}d ago")) }
        else { fallback.get() } } } })
    </time> }.boxed();
    (time, local_full)
}

/// Isolate timestamps mapped from multiple rows under a stable component scope.
pub(crate) fn relative_time_view<'a>(cx: &'a Cx, timestamp: &str, now: Signal<f64>) -> BoxView<'a> {
    let timestamp = timestamp.to_owned();
    let scoped = cx.keyed(&timestamp);
    view! {scoped=>scoped_relative_time(timestamp:timestamp,now:now)}.boxed()
}
#[component]
async fn scoped_relative_time(
    cx: &Cx,
    timestamp: String,
    now: Signal<f64>,
) -> topcoat::Result<impl View> {
    Ok(relative(cx, &timestamp, now).0)
}
pub(crate) fn absolute_time_view<'a>(cx: &'a Cx, timestamp: &str) -> BoxView<'a> {
    let timestamp = timestamp.to_owned();
    let scoped = cx.keyed(&timestamp);
    view! {scoped=>scoped_absolute_time(timestamp:timestamp)}.boxed()
}
#[component]
async fn scoped_absolute_time(cx: &Cx, timestamp: String) -> topcoat::Result<impl View> {
    Ok(absolute(cx, &timestamp))
}
