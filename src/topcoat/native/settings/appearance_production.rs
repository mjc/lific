use super::super::home_fixture;
use axum::http::StatusCode;

#[tokio::test]
async fn native_settings_appearance_renders_named_selected_controls_and_accent_swatches() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);

    let document = scraper::Html::parse_document(&html);
    let section = document
        .select(&scraper::Selector::parse("[data-native-appearance]").unwrap())
        .next()
        .expect("Settings exposes an appearance control group");
    let buttons = scraper::Selector::parse("button[data-native-appearance-choice]").unwrap();
    let controls = section
        .select(&buttons)
        .map(|button| {
            (
                button
                    .value()
                    .attr("data-native-appearance-choice")
                    .unwrap()
                    .to_owned(),
                button.value().attr("aria-pressed").unwrap_or("").to_owned(),
            )
        })
        .collect::<Vec<_>>();

    for (key, value) in [
        ("lific_theme", "system"),
        ("lific_accent", "indigo"),
        ("lific_density", "comfortable"),
        ("lific_font_scale", "md"),
        ("lific_motion", "system"),
    ] {
        assert!(
            controls.contains(&(format!("{key}:{value}"), "true".to_owned())),
            "default {key}={value} is exposed as selected"
        );
        assert_eq!(
            controls
                .iter()
                .filter(|(choice, selected)| choice.starts_with(&format!("{key}:"))
                    && selected == "true")
                .count(),
            1,
            "exactly one {key} choice is selected"
        );
    }
    for button in section.select(&buttons) {
        let choice = button
            .value()
            .attr("data-native-appearance-choice")
            .unwrap();
        let class = button.value().attr("class").unwrap_or("");
        let selected = button.value().attr("aria-pressed") == Some("true");
        if choice.starts_with("lific_accent:") {
            assert!(class.contains("size-8") && class.contains("rounded-full"));
            assert!(class.contains(if selected {
                "ring-2"
            } else {
                "hover:scale-110"
            }));
        } else {
            assert!(
                class.contains("rounded-md") && class.contains("px-3") && class.contains("py-1.5")
            );
            assert!(class.contains(if selected {
                "shadow-"
            } else {
                "text-[var(--text-muted)]"
            }));
        }
    }

    let class_bindings = section
        .select(&buttons)
        .map(|button| {
            (
                button
                    .value()
                    .attr("data-native-appearance-choice")
                    .unwrap()
                    .to_owned(),
                button
                    .value()
                    .attr("data-topcoat-bind:class")
                    .expect("selected styles remain reactive after hydration")
                    .to_owned(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let handlers = section
        .select(&buttons)
        .map(|button| {
            (
                button
                    .value()
                    .attr("data-native-appearance-choice")
                    .unwrap()
                    .to_owned(),
                button
                    .value()
                    .attr("data-topcoat-on:click")
                    .expect("each preference choice has its production click handler")
                    .to_owned(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/appearance.test.cjs",
        &serde_json::json!({
            "handlers": handlers,
            "class_bindings": class_bindings,
            "selection_handler": section
                .value()
                .attr("data-topcoat-on:mount")
                .expect("Settings hydrates persisted selected choices")
                .to_owned(),
            "signals": home_fixture::page_signals(&html),
            "theme_signal_id": "appearance-theme",
            "motion_signal_id": "appearance-motion",
            "initial_storage": {
                "lific_theme": "dark",
                "lific_accent": "violet",
                "lific_density": "compact",
                "lific_font_scale": "lg",
                "lific_motion": "invalid"
            },
            "preferences_factory": super::super::preferences::handler_factory().to_source(),
            "motion_factory": super::super::motion::handler_factory().to_source(),
        }),
    );
    assert_eq!(result["all_choices_applied"], true);

    for (accent, label) in [
        ("indigo", "Indigo"),
        ("teal", "Teal"),
        ("rose", "Rose"),
        ("amber", "Amber"),
        ("green", "Green"),
        ("violet", "Violet"),
    ] {
        let choice = format!("lific_accent:{accent}");
        let button = section
            .select(&buttons)
            .find(|button| button.value().attr("data-native-appearance-choice") == Some(&choice))
            .unwrap_or_else(|| panic!("missing accent swatch {accent}"));
        assert!(
            button
                .value()
                .attr("aria-label")
                .unwrap_or("")
                .contains(label)
        );
        assert!(
            button
                .value()
                .attr("style")
                .unwrap_or("")
                .contains("background-color")
        );
    }
}
