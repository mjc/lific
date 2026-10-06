//! Initial renders record selected static images for HTTP preload headers.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use axum::http::{HeaderMap, HeaderValue, header, request::Parts};
use topcoat::context::{Cx, try_request_context};

#[derive(Clone, Default)]
pub(crate) struct ImagePreloads(Arc<Mutex<BTreeSet<String>>>);

impl ImagePreloads {
    pub(crate) fn append_to(&self, headers: &mut HeaderMap) {
        let hints = self
            .0
            .lock()
            .expect("image preload lock")
            .iter()
            .map(|url| format!("<{url}>; rel=preload; as=image"))
            .reduce(|mut hints, hint| {
                hints.push_str(", ");
                hints.push_str(&hint);
                hints
            });
        if let Some(hints) = hints {
            headers.append(
                header::LINK,
                HeaderValue::from_str(&hints).expect("static image URLs are valid headers"),
            );
        }
    }
}

/// Initial HTTP renders record preloads; socket renders keep the same URLs.
pub(crate) fn image_url(cx: &Cx, logical_path: &str) -> String {
    let url = super::transport::mounted_url(cx, logical_path);
    if let Some(images) =
        try_request_context::<Parts>(cx).and_then(|parts| parts.extensions.get::<ImagePreloads>())
    {
        images
            .0
            .lock()
            .expect("image preload lock")
            .insert(url.clone());
    }
    url
}
