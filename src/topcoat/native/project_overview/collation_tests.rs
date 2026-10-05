use super::*;
#[test]
fn label_order_matches_pinned_master_chromium_default_english_and_swedish_oracle() {
    // Chromium149.0.7827.55 evaluating the actual master9683d38 derived body.
    // This proves this corpus, not universal ICU4X/Chromium version identity.
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("collation_oracle.json")).unwrap();
    let labels = oracle["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| Label {
            id: row["id"].as_i64().unwrap(),
            project_id: 1,
            name: row["name"].as_str().unwrap().into(),
            color: super::super::labels_model::DEFAULT_COLOR.into(),
        })
        .collect::<Vec<_>>();
    let usage = labels
        .iter()
        .map(|label| {
            (
                label.name.clone(),
                Usage {
                    issues: if label.name == "z" { 3 } else { 1 },
                    pages: 0,
                },
            )
        })
        .collect();
    for reference in oracle["results"].as_array().unwrap() {
        let settings: BrowserCollation =
            serde_json::from_value(reference["options"].clone()).unwrap();
        for (key, sort, filter) in [
            ("name", Sort::Name, ""),
            ("usage", Sort::Usage, ""),
            ("newest", Sort::Newest, ""),
            ("filtered", Sort::Name, "ECLA"),
        ] {
            let actual = visible(&labels, &usage, filter, sort, &settings)
                .unwrap()
                .iter()
                .map(|label| label.id)
                .collect::<Vec<_>>();
            let expected = reference[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_i64().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{} {key}", reference["requestedLocale"]);
        }
        let comparator = settings.comparator().unwrap();
        for pair in reference["pairs"].as_array().unwrap() {
            let actual =
                comparator.compare(pair["a"].as_str().unwrap(), pair["b"].as_str().unwrap());
            assert_eq!(actual as i8, pair["result"].as_i64().unwrap() as i8);
        }
    }
}
#[test]
fn malformed_locale_options_and_sort_never_fall_back_to_lexical_order() {
    for value in [
        "{}",
        "[]",
        r#"{"locale":"en-US","usage":"search","sensitivity":"variant","ignorePunctuation":false,"collation":"default","numeric":false,"caseFirst":"false"}"#,
    ] {
        assert!(BrowserCollation::from_wire(value).is_err());
    }
    assert!(BrowserCollation::from_wire(&"x".repeat(2049)).is_err());
    assert!(Sort::parse("oldest").is_err());
    let mut settings:BrowserCollation=serde_json::from_str(r#"{"locale":"bad locale","usage":"sort","sensitivity":"variant","ignorePunctuation":false,"collation":"default","numeric":false,"caseFirst":"false"}"#).unwrap();
    assert!(settings.comparator().is_err());
    settings.locale = "en-US".into();
    settings.collation = "not a collation".into();
    assert!(settings.comparator().is_err());
}
