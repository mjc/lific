//! Legacy hash URLs are untrusted browser input. Canonicalize them in Rust.

use topcoat::{
    context::Cx,
    runtime::{Event, expr, procedure, signal},
    view::Attributes,
};

pub(crate) fn logical_destination(hash: &str) -> Option<String> {
    if hash.len() > 4096
        || hash.chars().any(|character| {
            character.is_whitespace() || character.is_control() || character == '\\'
        })
    {
        return None;
    }
    let route = hash.strip_prefix('#')?;
    if !route.starts_with('/') || route.starts_with("//") {
        return None;
    }
    let path_and_query = route.split('#').next()?;
    let uri: axum::http::Uri = path_and_query.parse().ok()?;
    if uri.scheme().is_some() || uri.authority().is_some() {
        return None;
    }
    Some(route.to_owned())
}

#[procedure("/__native_home/bookmark")]
async fn native_home_bookmark(cx: &Cx, hash: String) -> topcoat::Result<Option<String>> {
    Ok(logical_destination(&hash).map(|path| super::transport::mounted_url(cx, &path)))
}

pub(crate) fn mount(cx: &Cx) -> Attributes {
    let initialized = signal(cx, || false);
    let handler = expr!(async |_event: Event| {
        if !initialized.get() {
            initialized.set(true);
            let hash = raw!("cx.hydrate(window.location.hash)", String::new());
            if !hash.is_empty() {
                let destination = native_home_bookmark(hash).await;
                if destination.is_some() {
                    let _destination = destination.unwrap();
                    raw!("window.location.replace(${_destination}.toString())", ());
                }
            }
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attributes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_hash_bookmarks_preserve_internal_path_query_and_inner_fragment() {
        for (hash, expected) in [
            ("#/ACC/issues", "/ACC/issues"),
            ("#/public/ACC/issues/ACC-1", "/public/ACC/issues/ACC-1"),
            (
                "#/ACC/issues?status=active#row",
                "/ACC/issues?status=active#row",
            ),
            ("#/", "/"),
        ] {
            assert_eq!(logical_destination(hash).as_deref(), Some(expected));
        }
    }

    #[test]
    fn legacy_hash_bookmarks_reject_external_ambiguous_and_oversized_destinations() {
        for hash in [
            "",
            "#row",
            "#https://other.example/",
            "#//other.example/",
            "#/\\other.example/",
            "#/ACC/issues\n",
            "#/ACC issues",
        ] {
            assert_eq!(logical_destination(hash), None, "{hash:?}");
        }
        assert_eq!(
            logical_destination(&format!("#/{}", "a".repeat(4096))),
            None
        );
    }
}
