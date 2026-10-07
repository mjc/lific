//! Shared rendering of the original theme-aware empty-state artwork.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

#[derive(Clone, Copy)]
pub(crate) enum Mascot {
    Reading,
    Writing,
}

pub(crate) fn render(cx: &Cx, mascot: Mascot, scale: f64) -> BoxView<'_> {
    let (path, width, height) = match mascot {
        Mascot::Reading => ("/__native_login/mascot.png", 487.0, 714.0),
        Mascot::Writing => ("/__native_signup/mascot.png", 567.0, 562.0),
    };
    let path = super::transport::mounted_url(cx, path);
    let width = (width * scale).round();
    let height = (height * scale).round();
    let style = format!(
        "width:{width}px;height:{height}px;mask:url({path}) center / contain no-repeat;-webkit-mask:url({path}) center / contain no-repeat"
    );
    view! { cx => <div aria-hidden="true" class="shrink-0 opacity-50 bg-[var(--text-faint)]" style=(style)></div> }.boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_state_artwork_keeps_master_source_dimensions_and_scale() {
        let cx = Cx::default();
        for (mascot, dimensions, source) in [
            (
                Mascot::Reading,
                "width:122px;height:179px",
                "__native_login/mascot.png",
            ),
            (
                Mascot::Writing,
                "width:142px;height:141px",
                "__native_signup/mascot.png",
            ),
        ] {
            let html = render(&cx, mascot, 0.25)
                .single()
                .await
                .unwrap()
                .render(&cx);
            assert!(html.contains(dimensions));
            assert!(html.contains(source));
            assert!(html.contains("aria-hidden=\"true\""));
        }
    }
}
