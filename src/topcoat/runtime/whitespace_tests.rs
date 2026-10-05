use super::*;
use topcoat::runtime::{Surrogated, expr};
fn expression_cases() -> Vec<serde_json::Value> {
    let mut cases = Vec::new();
    for (text, expected) in [
        ("", ""),
        (" plain ", "plain"),
        ("\u{feff} value \u{feff}", "value"),
        ("\u{85}value\u{85}", "\u{85}value\u{85}"),
        ("\u{feff}\u{85}x\u{85}\u{feff}", "\u{85}x\u{85}"),
        ("\u{2028}😀\u{2029}", "😀"),
        ("\tvalue\t", "value"),
        ("\nvalue\n", "value"),
        ("\u{b}value\u{b}", "value"),
        ("\u{c}value\u{c}", "value"),
        ("\rvalue\r", "value"),
        ("\u{a0}value\u{a0}", "value"),
        ("\u{1680}value\u{1680}", "value"),
        (
            "\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}value\u{200a}",
            "value",
        ),
        ("\u{202f}value\u{202f}", "value"),
        ("\u{205f}value\u{205f}", "value"),
        ("\u{3000}value\u{3000}", "value"),
        (" x\u{feff}y ", "x\u{feff}y"),
    ] {
        let text = text.to_owned();
        let (actual, js) = expr!(text.clone().trim_ecmascript()).into_evaluated_and_js();
        assert_eq!(actual, expected);
        cases.push(serde_json::json!({"source":js.to_source(),"wire":serde_json::to_value(actual.into_surrogate()).unwrap(),"expected":{"kind":"Return","value":{"type":"String","value":expected}}}));
    }
    cases
}

#[test]
fn ecmascript_trim_keeps_nel_and_removes_bom_with_actual_rust_wires() {
    let cases = expression_cases();
    if let Some(output) = std::env::var_os("LIFIC_WHITESPACE_COHERENCE_OUTPUT") {
        std::fs::write(output, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    }
}

#[test]
fn ecmascript_whitespace_predicate_is_shared_by_trim_and_initials() {
    for scalar in [
        '\t', '\n', '\u{b}', '\u{c}', '\r', ' ', '\u{a0}', '\u{1680}', '\u{2000}', '\u{2001}',
        '\u{2002}', '\u{2003}', '\u{2004}', '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}',
        '\u{2009}', '\u{200a}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}',
        '\u{feff}',
    ] {
        assert!(is_ecmascript_whitespace(scalar), "U+{:04X}", scalar as u32);
    }
    for scalar in ['\u{85}', '\0', '\u{180e}', '\u{200b}', 'a', '😀'] {
        assert!(!is_ecmascript_whitespace(scalar), "U+{:04X}", scalar as u32);
    }
    assert_eq!(
        trim_ecmascript("\u{feff}\u{85}x\u{85}\u{feff}"),
        "\u{85}x\u{85}"
    );
}

#[tokio::test]
async fn packaged_ecmascript_trim_matches_actual_rust_expression_wires_in_chromium() {
    let directory = tempfile::tempdir().unwrap();
    let output_path = directory.path().join("whitespace-cases.json");
    std::fs::write(
        &output_path,
        serde_json::to_vec(&expression_cases()).unwrap(),
    )
    .unwrap();
    let output = super::super::super::native::home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/whitespace.browser.test.cjs"
        ),
        "",
        "",
    )
    .env("LIFIC_WHITESPACE_COHERENCE_OUTPUT", &output_path)
    .output()
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "Packaged trim diverged from actual Rust expressions:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
