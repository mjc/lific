//! Main's error surface, shared by native page families.
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

pub(crate) const MASCOT: &[u8] = include_bytes!("assets/error-lizzy.png");

pub(crate) fn surface<'a>(
    cx: &'a Cx,
    title: &'static str,
    message: &str,
    actions: BoxView<'a>,
) -> BoxView<'a> {
    let message = message.to_owned();
    let mascot = super::transport::mounted_url(cx, "/__native_error/mascot.png");
    view! { cx => <div class="native-error-state w-full flex-1 h-full min-h-[55vh] relative overflow-hidden flex items-center" role="alert"><div class="native-error-state__copy relative z-10 max-w-[440px] pl-8 sm:pl-14 pr-6 py-12"><p class="native-error-state__title m-0 font-display text-title tracking-tight text-[var(--text)] leading-tight">(title)</p><p class="native-error-state__message m-0 mt-2 max-w-[42ch] text-body text-[var(--text-muted)] leading-relaxed">(message)</p><div class="native-error-state__actions flex items-center gap-2 mt-5">(actions)</div></div><div class="native-error-state__art pointer-events-none absolute right-0 bottom-[14%] z-0 rotate-[7deg] translate-x-[18%]" aria-hidden="true"><div class="w-[312px] h-[205px] shrink-0 opacity-50 bg-[var(--text-faint)]" style=(format!("mask:url({mascot}) center / contain no-repeat;-webkit-mask:url({mascot}) center / contain no-repeat"))></div></div></div> }.boxed()
}
