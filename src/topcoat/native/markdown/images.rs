//! Shared attachment image previews for private and published Markdown.

use super::super::browser;
use super::{Scope, render};
use topcoat::{
    context::Cx,
    runtime::{Event, expr, signal},
    view::{Attributes, BoxView, Unescaped, ViewExt, view},
};

pub(crate) fn private_view<'a>(cx: &'a Cx, source: &str, mentions: &[(&str, &str)]) -> BoxView<'a> {
    let rendered = render(cx, source, Scope::Private, mentions);
    let rendered = super::decorate_private_images(cx, &rendered);
    rendered_view(cx, source, rendered)
}

pub(crate) fn published_view<'a>(cx: &'a Cx, project: &str, source: &str) -> BoxView<'a> {
    rendered_view(cx, source, super::render_published(cx, project, source))
}

fn rendered_view<'a>(cx: &'a Cx, source: &str, rendered: String) -> BoxView<'a> {
    let view_cx = cx.keyed(source);
    let open = signal(&view_cx.keyed("preview-open"), || false);
    let original = signal(&view_cx.keyed("preview-original"), String::new);
    let alt = signal(&view_cx.keyed("preview-alt"), String::new);

    let click_browser = browser::bindings();
    let click_open = open.clone();
    let click_original = original.clone();
    let click_alt = alt.clone();
    let click = expr!(|event: Event| {
        click_browser.markdown_image_action(event, |event: Event, action: String| {
            if !click_browser.is_disposed() {
                let kind =
                    click_browser.json_string(action.clone(), "kind".to_owned(), "".to_owned());
                if kind == "open" {
                    click_original.set(click_browser.json_string(
                        action.clone(),
                        "original".to_owned(),
                        "".to_owned(),
                    ));
                    click_alt.set(click_browser.json_string(
                        action,
                        "alt".to_owned(),
                        "".to_owned(),
                    ));
                    click_open.set(true);
                    event.prevent_default();
                } else if kind == "close" {
                    if click_open.get() {
                        click_open.set(false);
                    }
                }
            }
        });
    });

    let mount_browser = browser::bindings();
    let escape_open = open.clone();
    let mount = expr!(|event: Event| {
        if !mount_browser.is_disposed() {
            mount_browser.markdown_image_fallback(event);
            mount_browser.window_listener("keydown".to_owned(), |event: Event| {
                if !mount_browser.is_disposed() {
                    if event.key == "Escape" {
                        if escape_open.get() {
                            event.prevent_default();
                            escape_open.set(false);
                        }
                    }
                }
            });
        }
    });

    let mut attributes = Attributes::with_capacity(2);
    attributes.insert(cx, "data-topcoat-on:click", click.into_evaluated_and_js().1);
    attributes.insert(cx, "data-topcoat-on:mount", mount.into_evaluated_and_js().1);

    view! {
        view_cx =>
        <div
            class="native-markdown-images tc-markdown"
            data-native-markdown-images=""
            (attributes)
        >
            (Unescaped::new_unchecked(rendered))
            <div
                :class=$(if open.get() {
                    "fixed inset-0 z-[1200] flex items-center justify-center p-8 bg-black/[0.78] cursor-zoom-out"
                } else {
                    "hidden fixed inset-0 z-[1200] flex items-center justify-center p-8 bg-black/[0.78] cursor-zoom-out"
                })
                :hidden=$(if open.get() { false } else { true })
                data-native-markdown-preview=""
                role="dialog"
                aria-modal="true"
                aria-label="Image preview"
                tabindex="-1"
            >
                <img
                    class="max-w-full max-h-[calc(100vh-4rem)] h-auto object-contain shadow-2xl"
                    :src=$(if open.get() { Some(original.get()) } else { None })
                    :alt=$(alt.get())
                >
            </div>
        </div>
    }
    .boxed()
}
