//! Pan, zoom, and fit controls for the dependency graph viewport.
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

#[derive(Clone)]
pub(super) struct State {
    pub(super) zoom: Signal<f64>,
    pub(super) pan_x: Signal<f64>,
    pub(super) pan_y: Signal<f64>,
    active: Signal<bool>,
    pointer_id: Signal<f64>,
    start_x: Signal<f64>,
    start_y: Signal<f64>,
    origin_x: Signal<f64>,
    origin_y: Signal<f64>,
}

pub(super) fn state(cx: &Cx) -> State {
    State {
        zoom: signal(cx, || 1.0_f64),
        pan_x: signal(cx, || 0.0_f64),
        pan_y: signal(cx, || 0.0_f64),
        active: signal(cx, || false),
        pointer_id: signal(cx, || 0.0_f64),
        start_x: signal(cx, || 0.0_f64),
        start_y: signal(cx, || 0.0_f64),
        origin_x: signal(cx, || 0.0_f64),
        origin_y: signal(cx, || 0.0_f64),
    }
}

pub(super) fn controls<'a>(cx: &'a Cx, state: &State) -> BoxView<'a> {
    let zoom_in_state = state.zoom.clone();
    let zoom_out_state = state.zoom.clone();
    let fit_zoom = state.zoom.clone();
    let fit_x = state.pan_x.clone();
    let fit_y = state.pan_y.clone();
    let zoom_out = view! {
        cx =>
        <button
            type="button"
            aria-label="Zoom out"
            class="size-9 rounded-md hover:bg-[var(--bg-subtle)]"
            data-native-graph-zoom="out"
            @click=$(|_event: Event| {
                let next = zoom_out_state.get() / 1.2;
                zoom_out_state.set(if next < 0.1 { 0.1 } else { next });
            })
        >
            "−"
        </button>
    }
    .boxed();
    let fit = view! {
        cx =>
        <button
            type="button"
            aria-label="Fit graph to view"
            class="size-9 rounded-md hover:bg-[var(--bg-subtle)]"
            data-native-graph-fit=""
            @click=$(|_event: Event| {
                let geometry = raw!(
                    "cx.hydrate((() => { const viewport=${_event}.inner.target.closest('[data-native-graph-viewport]'); const transform=viewport?.querySelector('[data-native-graph-transform]'); return [viewport?.clientWidth||0,viewport?.clientHeight||0,transform?.offsetWidth||1,transform?.offsetHeight||1]; })())",
                    (0.0, 0.0, 1.0, 1.0),
                );
                let viewport_width = geometry.0;
                let viewport_height = geometry.1;
                let width = geometry.2;
                let height = geometry.3;
                let padding_x = raw!(
                    "cx.hydrate(Math.floor((Number(${viewport_width}.toString())-Number(${viewport_width}.toString())/1.15)*0.5))",
                    0.0,
                );
                let padding_y = raw!(
                    "cx.hydrate(Math.floor((Number(${viewport_height}.toString())-Number(${viewport_height}.toString())/1.15)*0.5))",
                    0.0,
                );
                let fit_width = (viewport_width - padding_x * 2.0) / width;
                let fit_height = (viewport_height - padding_y * 2.0) / height;
                let fit_scale = if fit_width < fit_height {
                    fit_width
                } else {
                    fit_height
                };
                let fit_scale = if fit_scale < 0.1 {
                    0.1
                } else if fit_scale > 2.0 {
                    2.0
                } else {
                    fit_scale
                };
                fit_zoom.set(fit_scale);
                fit_x.set((viewport_width - width * fit_scale) / 2.0);
                fit_y.set((viewport_height - height * fit_scale) / 2.0);
            })
        >
            "⌗"
        </button>
    }.boxed();
    let zoom_in = view! {
        cx =>
        <button
            type="button"
            aria-label="Zoom in"
            class="size-9 rounded-md hover:bg-[var(--bg-subtle)]"
            data-native-graph-zoom="in"
            @click=$(|_event: Event| {
                let next = zoom_in_state.get() * 1.2;
                zoom_in_state.set(if next > 2.0 { 2.0 } else { next });
            })
        >
            "+"
        </button>
    }
    .boxed();

    view! {
        cx =>
        <div
            class="absolute left-3 top-3 z-10 flex gap-1 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-1"
        >
            (zoom_out)
            (fit)
            (zoom_in)
        </div>
    }
    .boxed()
}

pub(super) fn pan_surface_attrs(cx: &Cx, state: &State) -> Attributes {
    let down_active = state.active.clone();
    let down_pointer = state.pointer_id.clone();
    let down_start_x = state.start_x.clone();
    let down_start_y = state.start_y.clone();
    let down_origin_x = state.origin_x.clone();
    let down_origin_y = state.origin_y.clone();
    let down_pan_x = state.pan_x.clone();
    let down_pan_y = state.pan_y.clone();
    let move_active = state.active.clone();
    let move_pointer = state.pointer_id.clone();
    let move_start_x = state.start_x.clone();
    let move_start_y = state.start_y.clone();
    let move_origin_x = state.origin_x.clone();
    let move_origin_y = state.origin_y.clone();
    let move_pan_x = state.pan_x.clone();
    let move_pan_y = state.pan_y.clone();
    let up_active = state.active.clone();
    let up_pointer = state.pointer_id.clone();
    let abort_active = state.active.clone();
    let abort_pointer = state.pointer_id.clone();
    let handler = expr!(|_event: Event| {
        let _down = |_event: Event| {
            let button = raw!("cx.hydrate(${_event}.inner.button)", 0.0);
            let primary = raw!("cx.hydrate(${_event}.inner.isPrimary)", false);
            let interactive = raw!(
                "cx.hydrate(Boolean(${_event}.inner.target.closest('[data-native-graph-node],[data-native-graph-edge],a,button,input,select,textarea,[role=menu]')))",
                false
            );
            if button == 0.0 {
                if primary {
                    if !interactive {
                        down_active.set(true);
                        down_pointer.set(raw!("cx.hydrate(${_event}.inner.pointerId)", 0.0));
                        down_start_x.set(_event.client_x);
                        down_start_y.set(_event.client_y);
                        down_origin_x.set(down_pan_x.get());
                        down_origin_y.set(down_pan_y.get());
                        raw!("${_event}.inner.currentTarget.style.cursor='grabbing';", ());
                        raw!(
                            "${_event}.inner.currentTarget.setPointerCapture?.(${_event}.inner.pointerId);",
                            ()
                        );
                    }
                }
            }
        };
        let _move = |_event: Event| {
            if move_active.get() {
                if move_pointer.get() == _event.pointer_id {
                    move_pan_x.set(move_origin_x.get() + _event.client_x - move_start_x.get());
                    move_pan_y.set(move_origin_y.get() + _event.client_y - move_start_y.get());
                }
            }
        };
        let _up = |_event: Event| {
            if up_active.get() {
                if up_pointer.get() == _event.pointer_id {
                    up_active.set(false);
                    raw!("surface.style.cursor='grab';", ());
                }
            }
        };
        let _abort = |_event: Event| {
            raw!("surface.style.cursor='grab';", ());
            if abort_active.get() {
                abort_active.set(false);
                let _pointer = abort_pointer.get();
                raw!(
                    "const pointerId=Number(${_pointer}.toString());if(surface.hasPointerCapture?.(pointerId)){try{surface.releasePointerCapture(pointerId)}catch(error){if(error?.name!=='NotFoundError')throw error}}",
                    ()
                );
            }
        };
        raw!(
            "const surface=${_event}.inner.target;surface.style.cursor='grab';surface.style.touchAction='none';surface.addEventListener('pointerdown',event=>${_down}(cx.event(event)),{signal:cx.abortSignal});window.addEventListener('pointermove',event=>${_move}(cx.event(event)),{signal:cx.abortSignal});for(const name of ['pointerup','pointercancel','lostpointercapture'])window.addEventListener(name,event=>${_up}(cx.event(event)),{signal:cx.abortSignal});cx.abortSignal.addEventListener('abort',event=>${_abort}(cx.event(event)),{once:true});",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attrs
}
