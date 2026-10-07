//! Pan, zoom, and fit controls for the dependency graph viewport.
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

use super::super::icons::{self, UiIcon};

#[derive(Clone)]
pub(super) struct State {
    pub(super) zoom: Signal<f64>,
    pub(super) pan_x: Signal<f64>,
    pub(super) pan_y: Signal<f64>,
    pub(super) transition_ms: Signal<f64>,
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
        transition_ms: signal(cx, || 0.0_f64),
        active: signal(cx, || false),
        pointer_id: signal(cx, || 0.0_f64),
        start_x: signal(cx, || 0.0_f64),
        start_y: signal(cx, || 0.0_f64),
        origin_x: signal(cx, || 0.0_f64),
        origin_y: signal(cx, || 0.0_f64),
    }
}

#[derive(Clone, Copy)]
enum ZoomDirection {
    In,
    Out,
}

pub(super) fn controls<'a>(cx: &'a Cx, state: &State) -> BoxView<'a> {
    let zoom_out = zoom_button(cx, state, ZoomDirection::Out);
    let zoom_in = zoom_button(cx, state, ZoomDirection::In);
    let fit_zoom = state.zoom.clone();
    let fit_x = state.pan_x.clone();
    let fit_y = state.pan_y.clone();
    let fit_duration = state.transition_ms.clone();
    let fit = view! {
        cx =>
        <button
            type="button"
            aria-label="Fit graph to view"
            class="size-9 grid place-items-center rounded-lg bg-[var(--surface)] border border-[var(--border)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]"
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
                let raw_fit_scale = if fit_width < fit_height {
                    fit_width
                } else {
                    fit_height
                };
                let fit_scale = if raw_fit_scale < 0.1 {
                    0.1
                } else if raw_fit_scale > 2.0 {
                    2.0
                } else {
                    raw_fit_scale
                };
                let fit_pan_x = (viewport_width - width * fit_scale) / 2.0;
                let fit_pan_y = (viewport_height - height * fit_scale) / 2.0;
                fit_zoom.set(fit_scale);
                fit_x.set(fit_pan_x);
                fit_y.set(fit_pan_y);
                let reduced = raw!(
                    "cx.hydrate((() => {const motion=document.documentElement.getAttribute('data-motion');return motion==='reduced'||(!motion&&matchMedia('(prefers-reduced-motion: reduce)').matches)})())",
                    false,
                );
                fit_duration.set(if reduced { 0.0 } else { 200.0 });
            })
        >
            (icons::ui_icon(cx, UiIcon::FitView, 15))
        </button>
    }.boxed();
    view! {
        cx =>
        <div
            class="absolute right-3 bottom-3 z-10 flex items-center gap-1"
            data-native-graph-controls=""
        >
            <div
                class="flex items-center rounded-lg bg-[var(--surface)] border border-[var(--border)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] overflow-hidden"
                data-native-graph-zoom-controls=""
            >
                (zoom_out)
                (zoom_in)
            </div>
            (fit)
        </div>
    }
    .boxed()
}

fn zoom_button<'a>(cx: &'a Cx, state: &State, direction: ZoomDirection) -> BoxView<'a> {
    let zoom = state.zoom.clone();
    let pan_x = state.pan_x.clone();
    let pan_y = state.pan_y.clone();
    let transition_ms = state.transition_ms.clone();
    let zoom_in = matches!(direction, ZoomDirection::In);
    let factor = if zoom_in { 1.2 } else { 1.0 / 1.2 };
    let (action, label, icon) = if zoom_in {
        ("in", "Zoom in", UiIcon::ZoomIn)
    } else {
        ("out", "Zoom out", UiIcon::ZoomOut)
    };
    view! {
        cx =>
        <button
            type="button"
            aria-label=(label)
            class="size-9 grid place-items-center text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]"
            data-native-graph-zoom=(action)
            @click=$(|_event: Event| {
                let dimensions = raw!(
                    "cx.hydrate((() => {const viewport=${_event}.inner.target.closest('[data-native-graph-viewport]');return [viewport?.clientWidth||0,viewport?.clientHeight||0]})())",
                    (0.0, 0.0),
                );
                let current = zoom.get();
                let raw_zoom = current * factor;
                let next_zoom = if raw_zoom > 2.0 {
                    2.0
                } else if raw_zoom < 0.1 {
                    0.1
                } else {
                    raw_zoom
                };
                let center_x = dimensions.0 / 2.0;
                let center_y = dimensions.1 / 2.0;
                let next_x = center_x - (center_x - pan_x.get()) * next_zoom / current;
                let next_y = center_y - (center_y - pan_y.get()) * next_zoom / current;
                pan_x.set(next_x);
                pan_y.set(next_y);
                zoom.set(next_zoom);
                let reduced = raw!(
                    "cx.hydrate((() => {const motion=document.documentElement.getAttribute('data-motion');return motion==='reduced'||(!motion&&matchMedia('(prefers-reduced-motion: reduce)').matches)})())",
                    false,
                );
                transition_ms.set(if reduced { 0.0 } else { 150.0 });
            })
        >
            (icons::ui_icon(cx, icon, 15))
        </button>
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
    let initial_zoom = state.zoom.clone();
    let initial_x = state.pan_x.clone();
    let initial_y = state.pan_y.clone();
    let initial_duration = state.transition_ms.clone();
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
        let geometry = raw!(
            "cx.hydrate((() => {const surface=${_event}.inner.target;const viewport=surface.closest('[data-native-graph-viewport]');const transform=viewport?.querySelector('[data-native-graph-transform]');return [viewport?.clientWidth||0,viewport?.clientHeight||0,transform?.offsetWidth||1,transform?.offsetHeight||1]})())",
            (0.0, 0.0, 1.0, 1.0),
        );
        if geometry.0 > 0.0 {
            if geometry.1 > 0.0 {
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
                let raw_zoom = if fit_width < fit_height {
                    fit_width
                } else {
                    fit_height
                };
                let zoom = if raw_zoom < 0.1 {
                    0.1
                } else if raw_zoom > 2.0 {
                    2.0
                } else {
                    raw_zoom
                };
                let pan_x = (viewport_width - width * zoom) / 2.0;
                let pan_y = (viewport_height - height * zoom) / 2.0;
                initial_zoom.set(zoom);
                initial_x.set(pan_x);
                initial_y.set(pan_y);
            }
        }
        initial_duration.set(0.0);
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
