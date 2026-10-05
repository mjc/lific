//! Generic local Unicode string operations using existing string/vector wire values.
use topcoat::runtime::{
    StrSurrogate, StringSurrogate, Surrogate, Surrogated, UsizeSurrogate, VecSurrogate,
};

pub(crate) trait StrUnicodeExt {
    fn to_uppercase(&self) -> StringSurrogate;
    // The typed usize carries target width to the browser Vec constructor.
    // Its numerical value is not an index or application setting.
    fn unicode_scalars(&self, target: UsizeSurrogate) -> VecSurrogate<String>;
}
impl StrUnicodeExt for StrSurrogate {
    fn to_uppercase(&self) -> StringSurrogate {
        Surrogate::into_real(self).to_uppercase().into_surrogate()
    }
    fn unicode_scalars(&self, _target: UsizeSurrogate) -> VecSurrogate<String> {
        Surrogate::into_real(self)
            .chars()
            .map(|scalar| scalar.to_string())
            .collect::<Vec<_>>()
            .into_surrogate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use topcoat::runtime::{StringSurrogate, Surrogate, Surrogated, expr};

    #[test]
    fn string_operations_match_rust_and_export_production_expr_coherence_cases() {
        let mut cases = Vec::new();
        for text in [
            "",
            "ascii",
            "Straße",
            "ﬃ",
            "iıİ",
            "σς",
            "😀a",
            "𝕒ß",
            "a\u{301}😀",
        ] {
            let owned = text.to_owned().into_surrogate();
            assert_eq!(owned.to_uppercase().into_real(), text.to_uppercase());
            let expected: Vec<String> = text.chars().map(|scalar| scalar.to_string()).collect();
            assert_eq!(
                owned.unicode_scalars(0_usize.into_surrogate()).into_real(),
                expected
            );
            let text_owned = text.to_owned();
            let (value, js) =
                expr!(text_owned.clone().unicode_scalars(0_usize)).into_evaluated_and_js();
            assert_eq!(value, expected);
            cases.push(serde_json::json!({
                "source": js.to_source(),
                "wire": serde_json::to_value(value.into_surrogate()).unwrap(),
                "expected": {"kind":"Return","value":{"type":"Sequence","value": expected.iter().map(|value| serde_json::json!({"type":"String","value":value})).collect::<Vec<_>>()}}
            }));
            let (value, js) = expr!(text_owned.clone().to_uppercase()).into_evaluated_and_js();
            assert_eq!(value, text.to_uppercase());
            cases.push(serde_json::json!({
                "source":js.to_source(),
                "wire":serde_json::to_value(value.into_surrogate()).unwrap(),
                "expected":{"kind":"Return","value":{"type":"String","value":text.to_uppercase()}}
            }));
        }
        if let Some(output) = std::env::var_os("LIFIC_STRING_COHERENCE_OUTPUT") {
            std::fs::write(output, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
        }
    }

    #[test]
    fn generic_methods_typecheck_without_server_calls() {
        let expression =
            expr!(|text: StringSurrogate| text.to_uppercase().unicode_scalars(0_usize));
        let source = expression.into_evaluated_and_js().1.to_source();
        assert!(source.contains(".to_uppercase("));
        assert!(source.contains(".unicode_scalars("));
        assert!(!source.contains("fetch("));
    }
}
