use topcoat::{
    context::Cx,
    runtime::{Signal, signal},
};

use super::model::ImportResult;

#[derive(Clone)]
pub(crate) struct State {
    pub(crate) account: i64,
    pub(crate) fingerprint: String,
    pub(crate) file_name: Signal<String>,
    pub(crate) file_size: Signal<usize>,
    pub(crate) file_error: Signal<String>,
    pub(crate) confirmed: Signal<bool>,
    pub(crate) phase: Signal<String>,
    pub(crate) progress: Signal<Option<usize>>,
    pub(crate) error: Signal<String>,
    pub(crate) result: Signal<Option<ImportResult>>,
}

impl State {
    pub(crate) fn new(cx: &Cx, account: i64, fingerprint: String) -> Self {
        Self {
            account,
            fingerprint,
            file_name: signal(cx, String::new),
            file_size: signal(cx, || 0_usize),
            file_error: signal(cx, String::new),
            confirmed: signal(cx, || false),
            phase: signal(cx, || "idle".to_owned()),
            progress: signal(cx, || None::<usize>),
            error: signal(cx, String::new),
            result: signal(cx, || None::<ImportResult>),
        }
    }
}

pub(crate) fn session_fingerprint(token: Option<&str>) -> String {
    token
        .map(|token| {
            let mut bytes = b"lific-native-archive-import-session\0".to_vec();
            bytes.extend_from_slice(token.as_bytes());
            crate::auth::sha256_hex(&bytes)
        })
        .unwrap_or_default()
}
