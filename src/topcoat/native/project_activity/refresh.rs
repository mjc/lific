//! Document-owned focus scheduling and project publication invalidations.
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Signal, StringSurrogate, expr},
    view::{Attributes, BoxView, ViewExt, emit, live},
};

pub(super) fn mount(
    cx: &Cx,
    revision: Signal<usize>,
    loading_more: Signal<bool>,
    timezone: Signal<String>,
) -> Attributes {
    let path = super::super::transport::mounted_url(cx, "/__native_project_activity/body");
    let failed_loading = loading_more.clone();
    let handler = expr!(|_event: Event| {
        raw!(
            "const owner=${_event}.inner.target; let debounce,retry;",
            ()
        );
        let _run = || {
            let hidden = raw!("cx.hydrate(document.hidden)", false);
            if !hidden {
                if loading_more.get() {
                    raw!(
                        "clearTimeout(retry); retry=setTimeout(()=>owner.nativeActivityRun(),2000);",
                        ()
                    );
                } else {
                    revision.increment();
                }
            };
        };
        raw!("owner.nativeActivityRun=()=>${_run}();", ());
        let _schedule = |_event: Event| {
            raw!(
                "clearTimeout(debounce); debounce=setTimeout(()=>${_run}(),50);",
                ()
            );
        };
        raw!(
            "owner.nativeActivitySchedule=()=>${_schedule}(${_event});",
            ()
        );
        let _failed = |failed_path: StringSurrogate| {
            if failed_path == path {
                failed_loading.set(false);
            }
        };
        timezone.set(raw!(
            "cx.hydrate(Intl.DateTimeFormat().resolvedOptions().timeZone)",
            String::new()
        ));
        raw!(
            "window.addEventListener('focus',event=>${_schedule}(cx.event(event)),{signal:cx.abortSignal}); document.addEventListener('visibilitychange',event=>${_schedule}(cx.event(event)),{signal:cx.abortSignal}); owner.addEventListener('topcoat:render-error',event=>${_failed}(cx.hydrate(event.detail.path)),{signal:cx.abortSignal}); cx.abortSignal.addEventListener('abort',()=>{clearTimeout(debounce);clearTimeout(retry);},{once:true});",
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

pub(super) fn live(cx: &Cx, account: i64, project: i64) -> BoxView<'_> {
    let context = cx.clone();
    let mut events = app_context::<crate::realtime::RealtimeHub>(cx).subscribe();
    let connected = super::super::super::runtime::connected(cx);
    live!{
        cx =>
        let token = emit! {
            <span hidden="hidden" data-native-project-activity-events=""></span>
        }?;
        if !connected {
            return Ok(token);
        }
        loop {
            let relevant = match events.recv().await {
                Ok(message) => matches!(
                        message.event,
                        crate::realtime::RealtimeEvent::ResyncRequired,
                    )
                    || message.event.project_id() == Some(project),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => true,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(token),
            };
            if relevant {
                let caller = super::super::session::read_for_refresh(
                    &context,
                    super::super::context::caller(&context),
                )?;
                let user = super::super::session::read_for_refresh(
                    &context,
                    crate::api::require_user(&caller.identity),
                )?;
                if user.id != account {
                    return Err(topcoat::router::error::forbidden().into());
                }
                let _updated = emit! {
                    <span
                        hidden="hidden"
                        @mount=$(|_event: Event| {
                            raw!(
                                "${_event}.inner.target.closest('[data-native-project-activity]').nativeActivitySchedule();",
                                (),
                            );
                        })
                    ></span>
                }?;
            }
        }
    }.boxed()
}
