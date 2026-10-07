use super::*;
use topcoat::{runtime::expr, view::Attributes};

pub(super) fn mount(cx: &Cx, close: Signal<String>) -> Attributes {
    let dragging = signal(cx, || false);
    let pointer = signal(cx, || 0.0);
    let start = signal(cx, || 0.0);
    let time = signal(cx, || 0.0);
    let distance = signal(cx, || 0.0);
    let height = signal(cx, || 0.0);
    let down_dragging = dragging.clone();
    let down_pointer = pointer.clone();
    let down_start = start.clone();
    let down_time = time.clone();
    let down_distance = distance.clone();
    let down_height = height.clone();
    let move_dragging = dragging.clone();
    let move_pointer = pointer.clone();
    let move_start = start;
    let move_distance = distance.clone();
    let handler = expr!(|_event: Event| {
        raw!(
            "const grab=${_event}.inner.target; const sheet=grab.closest('[data-native-issue-peek]'); let finish;",
            ()
        );
        let _down = |_event: Event| {
            let mobile = raw!("cx.hydrate(innerWidth<768)", false);
            let primary = raw!("cx.hydrate(${_event}.inner.isPrimary)", false);
            let kind = raw!("cx.hydrate(${_event}.inner.pointerType)", String::new());
            let interactive = raw!(
                "cx.hydrate(Boolean(${_event}.inner.target.closest('button,a,input,textarea,select')))",
                false
            );
            let touch = if kind == "touch" { true } else { kind == "pen" };
            if mobile {
                if primary {
                    if touch {
                        if !interactive {
                            down_dragging.set(true);
                            down_pointer.set(raw!("cx.hydrate(${_event}.inner.pointerId)", 0.0));
                            down_start.set(raw!("cx.hydrate(${_event}.inner.clientY)", 0.0));
                            down_time.set(raw!("cx.hydrate(performance.now())", 0.0));
                            down_distance.set(0.0);
                            let measured =
                                raw!("cx.hydrate(sheet.getBoundingClientRect().height)", 0.0);
                            down_height.set(if measured > 0.0 { measured } else { 480.0 });
                            raw!("clearTimeout(finish); sheet.style.transition='none';", ());
                        }
                    }
                }
            }
        };
        let _move = |_event: Event| {
            let id = raw!("cx.hydrate(${_event}.inner.pointerId)", 0.0);
            if move_dragging.get() {
                if id == move_pointer.get() {
                    let delta = raw!("cx.hydrate(${_event}.inner.clientY)", 0.0) - move_start.get();
                    let dy = if delta > 0.0 { delta } else { 0.0 };
                    move_distance.set(dy);
                    _event.prevent_default();
                    raw!(
                        "sheet.style.transform='translateY('+${dy}.toString()+'px)';",
                        ()
                    );
                }
            }
        };
        let _up = |_event: Event| {
            let id = raw!("cx.hydrate(${_event}.inner.pointerId)", 0.0);
            if dragging.get() {
                if id == pointer.get() {
                    dragging.set(false);
                    let cancelled =
                        raw!("cx.hydrate(${_event}.inner.type==='pointercancel')", false);
                    let elapsed = raw!("cx.hydrate(performance.now())", 0.0) - time.get();
                    let elapsed = if elapsed > 1.0 { elapsed } else { 1.0 };
                    let fast = if distance.get() > 24.0 {
                        distance.get() / elapsed > 0.45
                    } else {
                        false
                    };
                    let threshold = height.get().clone() * 0.28;
                    let enough = if distance.get() > threshold {
                        true
                    } else {
                        fast
                    };
                    let dismiss = if cancelled { false } else { enough };
                    if dismiss {
                        let _closed = || {
                            close.set("".to_owned());
                        };
                        let _slide = height.get() + 40.0;
                        raw!(
                            "sheet.style.transition='transform 170ms cubic-bezier(.2,.8,.3,1)'; sheet.style.transform='translateY('+${_slide}.toString()+'px)'; finish=setTimeout(()=>{sheet.style.visibility='hidden';${_closed}();},170);",
                            ()
                        );
                    } else {
                        raw!(
                            "sheet.style.transition='transform 170ms cubic-bezier(.2,.8,.3,1)';sheet.style.transform='translateY(0)';finish=setTimeout(()=>{sheet.style.transition='';sheet.style.transform='';},200);",
                            ()
                        );
                    }
                }
            }
        };
        raw!(
            "grab.addEventListener('pointerdown',event=>${_down}(cx.event(event)),{signal:cx.abortSignal});window.addEventListener('pointermove',event=>${_move}(cx.event(event)),{signal:cx.abortSignal,passive:false});for(const name of ['pointerup','pointercancel'])window.addEventListener(name,event=>${_up}(cx.event(event)),{signal:cx.abortSignal,capture:true});cx.abortSignal.addEventListener('abort',()=>clearTimeout(finish),{once:true});",
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
