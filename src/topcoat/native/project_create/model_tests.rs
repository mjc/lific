use super::*;

#[test]
fn generated_prefix_stops_on_manual_edit_and_clearing_name_keeps_last_prefix() {
    assert_eq!(name_changed("Half-Life 3", "", false), "HALFL");
    assert_eq!(name_changed("New name", "OWN", true), "OWN");
    assert_eq!(name_changed("", "HALFL", false), "HALFL");
    assert_eq!(name_changed("   ", "HALFL", false), "");
    assert_eq!(generated_identifier("ß é1 🦀ſﬃ"), "SS1SF");
    assert_eq!(generated_identifier("123456"), "12345");
}

#[test]
fn preview_uses_trimmed_uppercase_prefix_and_empty_fallback() {
    assert_eq!(preview(" acc "), "ACC-1");
    assert_eq!(preview("\u{feff} \u{feff}"), "PRO-1");
}

#[test]
fn submit_trim_matches_ecmascript_whitespace_oracle() {
    // Expected by pinned master Node String.trim(), not Unicode whitespace.
    for character in ['\u{feff}', '\u{2028}', '\u{2029}', '\u{00a0}'] {
        assert_eq!(trim(&format!("{character}ACC{character}")), "ACC");
    }
    for character in ['\u{0085}', '\u{180e}'] {
        let value = format!("{character}ACC{character}");
        assert_eq!(trim(&value), value);
    }
}

#[test]
fn submit_normalizes_fields_without_mutating_draft_or_group_and_touch_state() {
    let draft = Draft {
        name: " Project ".into(),
        identifier: " acc ".into(),
        identifier_touched: true,
        description: " Notes ".into(),
        emoji: " 🦀 ".into(),
        lead: Some(42),
        group: Some(7),
        error: String::new(),
    };
    let input = draft.input().unwrap();
    assert_eq!(
        (
            input.name.as_str(),
            input.identifier.as_str(),
            input.description.as_str()
        ),
        ("Project", "ACC", "Notes")
    );
    assert_eq!(input.emoji.as_deref(), Some("🦀"));
    assert_eq!(input.lead_user_id, Some(42));
    let roundtrip = Draft::from_wire(draft.wire());
    assert_eq!(roundtrip, draft);
    assert_eq!(draft.name, " Project ");
    assert!(
        Draft {
            identifier: "A".into(),
            ..Default::default()
        }
        .input()
        .is_err()
    );
    assert!(
        Draft {
            name: "A".into(),
            ..Default::default()
        }
        .input()
        .is_err()
    );
    let blank_icon = Draft {
        name: "A".into(),
        identifier: "A".into(),
        emoji: " ".into(),
        ..Default::default()
    }
    .input()
    .unwrap();
    assert_eq!(blank_icon.emoji, None);
    assert_eq!(blank_icon.lead_user_id, None);
}

#[test]
fn emoji_catalog_retains_pinned_order_logo_and_exact_1314_options() {
    let all = emojis("");
    assert_eq!(all.len(), 1314);
    assert_eq!(
        (
            all[0].value.as_str(),
            all[0].name.as_str(),
            all[0].group.as_str()
        ),
        ("lific:logo", "lific logo", "Lific")
    );
    assert_eq!(all[1].value, "👓");
    assert_eq!(all.last().unwrap().value, "🔲");
    assert!(!all.iter().any(|option| option.group == "Flags"));
    assert_eq!(emojis("LoGo"), vec![all[0].clone()]);
    assert_eq!(emojis("  "), all);
    // Master checks trimmed emptiness but searches the original query.
    assert!(emojis(" logo ").is_empty());
    assert!(emojis("nonexistentoriginalpickeremoji").is_empty());
}

#[test]
fn lucide_catalog_uses_all_sorted_pinned_names_and_case_insensitive_search() {
    let all = icons("");
    assert_eq!(all.len(), 1937);
    assert_eq!(all[0], "AArrowDown");
    assert_eq!(all.last().unwrap(), "ZoomOut");
    assert!(all.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(icons("fOlDeR").contains(&"Folder".to_owned()));
    assert_eq!(icons(" \t "), all);
    assert!(icons(" Folder ").is_empty());
}

#[test]
fn grid_uses_eight_columns_36px_rows_280px_viewport_and_two_row_overscan() {
    assert_eq!(
        grid_window(8, 0),
        GridWindow {
            total_height: 36,
            virtualized: false,
            start: 0,
            end: 8,
            offset: 0
        }
    );
    assert!(!grid_window(56, 0).virtualized);
    assert!(grid_window(57, 0).virtualized);
    assert_eq!(
        grid_window(1000, 360),
        GridWindow {
            total_height: 4500,
            virtualized: true,
            start: 64,
            end: 160,
            offset: 288
        }
    );
    assert_eq!(
        grid_window(0, usize::MAX),
        GridWindow {
            total_height: 0,
            virtualized: false,
            start: 0,
            end: 0,
            offset: 0
        }
    );
    let last = grid_window(1314, usize::MAX);
    assert_eq!(last.end, 1314);
    assert!(last.start < last.end);
    assert!(last.offset < last.total_height);
}

#[test]
fn select_position_flips_only_when_above_fits_and_clamps_horizontal_edges() {
    let cases = [
        (
            MenuGeometry {
                trigger: TriggerRect {
                    top: 300.0,
                    bottom: 340.0,
                    left: 200.0,
                    width: 120.0,
                },
                menu: MenuSize {
                    height: 100.0,
                    width: 240.0,
                },
                viewport: Viewport {
                    height: 800.0,
                    width: 600.0,
                },
            },
            MenuPosition {
                top: 344.0,
                left: 200.0,
                width: 120.0,
            },
        ),
        (
            MenuGeometry {
                trigger: TriggerRect {
                    top: 700.0,
                    bottom: 740.0,
                    left: 550.0,
                    width: 120.0,
                },
                menu: MenuSize {
                    height: 200.0,
                    width: 240.0,
                },
                viewport: Viewport {
                    height: 800.0,
                    width: 600.0,
                },
            },
            MenuPosition {
                top: 496.0,
                left: 352.0,
                width: 120.0,
            },
        ),
        (
            MenuGeometry {
                trigger: TriggerRect {
                    top: 20.0,
                    bottom: 60.0,
                    left: -20.0,
                    width: 120.0,
                },
                menu: MenuSize {
                    height: 500.0,
                    width: 640.0,
                },
                viewport: Viewport {
                    height: 400.0,
                    width: 360.0,
                },
            },
            MenuPosition {
                top: 64.0,
                left: 8.0,
                width: 120.0,
            },
        ),
        (
            MenuGeometry {
                trigger: TriggerRect {
                    top: 300.125,
                    bottom: 340.375,
                    left: 200.625,
                    width: 120.875,
                },
                menu: MenuSize {
                    height: 100.25,
                    width: 240.5,
                },
                viewport: Viewport {
                    height: 800.75,
                    width: 600.25,
                },
            },
            MenuPosition {
                top: 344.375,
                left: 200.625,
                width: 120.875,
            },
        ),
        (
            MenuGeometry {
                trigger: TriggerRect {
                    top: 700.875,
                    bottom: 740.9375,
                    left: 550.25,
                    width: 120.625,
                },
                menu: MenuSize {
                    height: 200.125,
                    width: 240.375,
                },
                viewport: Viewport {
                    height: 800.5,
                    width: 600.75,
                },
            },
            MenuPosition {
                top: 496.75,
                left: 352.375,
                width: 120.625,
            },
        ),
        (
            MenuGeometry {
                trigger: TriggerRect {
                    top: 212.25,
                    bottom: 252.5,
                    left: -0.125,
                    width: 120.625,
                },
                menu: MenuSize {
                    height: 200.25,
                    width: 240.375,
                },
                viewport: Viewport {
                    height: 400.0,
                    width: 600.75,
                },
            },
            MenuPosition {
                top: 8.0,
                left: 8.0,
                width: 120.625,
            },
        ),
        (
            MenuGeometry {
                trigger: TriggerRect {
                    top: 212.25,
                    bottom: 252.5,
                    left: 100.125,
                    width: 120.625,
                },
                menu: MenuSize {
                    height: 200.375,
                    width: 240.375,
                },
                viewport: Viewport {
                    height: 400.0,
                    width: 600.75,
                },
            },
            MenuPosition {
                top: 256.5,
                left: 100.125,
                width: 120.625,
            },
        ),
    ];
    for (geometry, expected) in cases {
        assert_eq!(menu_position(geometry), expected, "{geometry:?}");
    }
}
