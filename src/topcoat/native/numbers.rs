//! Browser-localized numbers with a populated initial fallback.
use topcoat::{
    context::Cx,
    runtime::{Event, signal},
    view::{BoxView, View, ViewExt, component, view},
};

fn content(cx: &Cx, count: i64) -> BoxView<'_> {
    let text = signal(cx, || localized_count(count));
    view! { cx => <span @mount=$(|_event: Event| {
        text.set(raw!("cx.hydrate(Number(${count}.toString()).toLocaleString())",String::new()));
    })>$(text.get())</span> }
    .boxed()
}

fn localized_count(count: i64) -> String {
    let digits = count.to_string();
    let mut output = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0
            && (digits.len() - index).is_multiple_of(3)
            && ch.is_ascii_digit()
            && !output.ends_with('-')
        {
            output.push(',');
        }
        output.push(ch);
    }
    output
}

pub(crate) fn count(cx: &Cx, value: i64) -> BoxView<'_> {
    let scoped = cx.keyed(value);
    view! {scoped=>scoped_number(value:value)}.boxed()
}
#[component]
async fn scoped_number(cx: &Cx, value: i64) -> topcoat::Result<impl View> {
    Ok(content(cx, value))
}
