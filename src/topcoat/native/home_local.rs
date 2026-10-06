//! Browser-local Home presentation from the pinned master Home.svelte and recents.ts.
//! These inputs describe this browser; they never select a caller or grant access.

use chrono::{DateTime, FixedOffset, TimeDelta, Timelike};
use serde::{Deserialize, Serialize};

use crate::db::models::AuthUser;

const MAX_INPUT_BYTES: usize = 128 * 1024;
const MAX_LOCALE_CHARACTERS: usize = 128;
const STORED_RECENTS: usize = 15;
const DISPLAY_RECENTS: usize = 8;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BrowserInputs {
    epoch_milliseconds: i64,
    timezone_offset_minutes: i32,
    locale: String,
    stored_value: Option<String>,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum InputError {
    #[error("browser inputs exceed the size limit")]
    TooLarge,
    #[error("browser inputs have an invalid shape")]
    Malformed,
    #[error("browser locale exceeds the length limit")]
    Locale,
    #[error("browser timezone offset is out of range")]
    Timezone,
    #[error("browser date is out of range")]
    Date,
}

impl BrowserInputs {
    pub(crate) fn parse(raw: &str) -> Result<Self, InputError> {
        if raw.len() > MAX_INPUT_BYTES {
            return Err(InputError::TooLarge);
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct WireInputs {
            epoch_milliseconds: i64,
            timezone_offset_minutes: i32,
            locale: String,
            stored_value: Option<String>,
        }
        let wire: WireInputs = serde_json::from_str(raw).map_err(|_| InputError::Malformed)?;
        let inputs = Self {
            epoch_milliseconds: wire.epoch_milliseconds,
            timezone_offset_minutes: wire.timezone_offset_minutes,
            locale: wire.locale,
            stored_value: wire.stored_value,
        };
        if inputs.locale.chars().count() > MAX_LOCALE_CHARACTERS {
            return Err(InputError::Locale);
        }
        // JavaScript getTimezoneOffset measures minutes west of UTC; chrono
        // FixedOffset measures seconds east. Its valid offsets exclude ±24h.
        let offset_seconds = inputs
            .timezone_offset_minutes
            .checked_neg()
            .and_then(|minutes| minutes.checked_mul(60))
            .ok_or(InputError::Timezone)?;
        let offset = FixedOffset::east_opt(offset_seconds).ok_or(InputError::Timezone)?;
        let utc =
            DateTime::from_timestamp_millis(inputs.epoch_milliseconds).ok_or(InputError::Date)?;
        // with_timezone permits a local date beyond chrono's representable
        // range. Check it here so date formatting cannot panic later.
        utc.naive_utc()
            .checked_add_signed(TimeDelta::seconds(i64::from(offset.local_minus_utc())))
            .ok_or(InputError::Date)?;
        Ok(inputs)
    }

    pub(crate) fn epoch_milliseconds(&self) -> i64 {
        self.epoch_milliseconds
    }

    pub(crate) fn locale(&self) -> &str {
        &self.locale
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GreetingIcon {
    Moon,
    Sunrise,
    Sun,
    Sunset,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Greeting {
    pub(crate) text: String,
    pub(crate) date_label: String,
    pub(crate) icon: GreetingIcon,
}

pub(crate) fn local_greeting(inputs: &BrowserInputs, user: Option<&AuthUser>) -> Greeting {
    let utc = DateTime::from_timestamp_millis(inputs.epoch_milliseconds)
        .expect("BrowserInputs validates its timestamp");
    let offset = FixedOffset::east_opt(-inputs.timezone_offset_minutes * 60)
        .expect("BrowserInputs validates its timezone offset");
    let local = utc.with_timezone(&offset);
    let (text, icon) = match local.hour() {
        0..5 => ("Good night", GreetingIcon::Moon),
        5..12 => ("Good morning", GreetingIcon::Sunrise),
        12..17 => ("Good afternoon", GreetingIcon::Sun),
        17..21 => ("Good evening", GreetingIcon::Sunset),
        _ => ("Good night", GreetingIcon::Moon),
    };
    let text = match user {
        Some(user) => {
            // JavaScript || tests emptiness, not trimmed emptiness.
            let name = if user.display_name.is_empty() {
                &user.username
            } else {
                &user.display_name
            };
            format!("{text}, {name}")
        }
        None => text.to_owned(),
    };
    Greeting {
        text,
        // Master explicitly selects en-US regardless of the browser locale.
        // chrono's default weekday/month names are English.
        date_label: local.format("%A, %B %-d").to_string(),
        icon,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RecentType {
    Issue,
    Page,
    Plan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentEntry {
    #[serde(rename = "type")]
    pub(crate) kind: RecentType,
    pub(crate) route_id: String,
    pub(crate) identifier: String,
    pub(crate) title: String,
    pub(crate) project: String,
    pub(crate) ts: i64,
}

/// A logical app destination. Its route still performs normal server authorization.
pub(crate) fn recent_route(entry: &RecentEntry) -> Option<String> {
    crate::db::queries::validate_project_identifier(&entry.project).ok()?;
    match entry.kind {
        RecentType::Issue => {
            let (project, sequence) = entry.route_id.split_once('-')?;
            if project != entry.project || !positive_id(sequence) {
                return None;
            }
        }
        RecentType::Page | RecentType::Plan => {
            if !positive_id(&entry.route_id) {
                return None;
            }
        }
    }
    let segment = match entry.kind {
        RecentType::Issue => "issues",
        RecentType::Page => "pages",
        RecentType::Plan => "plans",
    };
    Some(format!("/{}/{segment}/{}", entry.project, entry.route_id))
}

fn positive_id(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<i64>().is_ok_and(|id| id > 0)
}

pub(crate) fn stored_recents(inputs: &BrowserInputs) -> Vec<RecentEntry> {
    let Some(raw) = &inputs.stored_value else {
        return Vec::new();
    };
    let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(raw) else {
        return Vec::new();
    };
    // A malformed row does not discard the valid visits around it. Neither
    // loading nor display sorts by ts: master preserves the stored list order.
    entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<RecentEntry>(entry).ok())
        .filter(|entry| recent_route(entry).is_some())
        .take(STORED_RECENTS)
        .collect()
}

pub(crate) fn display_recents(entries: &[RecentEntry]) -> Vec<RecentEntry> {
    entries
        .iter()
        .filter(|entry| recent_route(entry).is_some())
        .take(DISPLAY_RECENTS)
        .cloned()
        .collect()
}

/// The visit timestamp comes from BrowserInputs, never the server's clock.
pub(crate) fn record_recent(entries: &[RecentEntry], visit: RecentEntry) -> Vec<RecentEntry> {
    if recent_route(&visit).is_none() {
        return entries
            .iter()
            .filter(|entry| recent_route(entry).is_some())
            .take(STORED_RECENTS)
            .cloned()
            .collect();
    }
    std::iter::once(&visit)
        .chain(
            entries
                .iter()
                // Intentionally no project key: master dedupes type + routeId.
                .filter(|entry| entry.kind != visit.kind || entry.route_id != visit.route_id)
                .filter(|entry| recent_route(entry).is_some()),
        )
        .take(STORED_RECENTS)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::{Value, json};

    use super::*;

    fn epoch(hour: u32) -> i64 {
        Utc.with_ymd_and_hms(2026, 10, 2, hour, 0, 0)
            .unwrap()
            .timestamp_millis()
    }

    fn raw(epoch_milliseconds: i64, offset: i32, locale: &str, stored: Option<&str>) -> String {
        json!({
            "epochMilliseconds": epoch_milliseconds,
            "timezoneOffsetMinutes": offset,
            "locale": locale,
            "storedValue": stored,
        })
        .to_string()
    }

    fn inputs(hour: u32, offset: i32, stored: Option<&str>) -> BrowserInputs {
        BrowserInputs::parse(&raw(epoch(hour), offset, "fr-FR", stored)).unwrap()
    }

    fn recent(kind: RecentType, route_id: &str, project: &str, ts: i64) -> RecentEntry {
        RecentEntry {
            kind,
            route_id: route_id.into(),
            identifier: format!("{project}-DISPLAY-{route_id}"),
            title: format!("Title {route_id}"),
            project: project.into(),
            ts,
        }
    }

    #[test]
    fn browser_inputs_reject_oversized_shapes_and_bound_locale_by_characters() {
        let input = raw(epoch(12), 420, &"é".repeat(MAX_LOCALE_CHARACTERS), None);
        let parsed = BrowserInputs::parse(&input).unwrap();
        assert_eq!(parsed.epoch_milliseconds(), epoch(12));
        assert_eq!(parsed.locale(), "é".repeat(MAX_LOCALE_CHARACTERS));
        assert_eq!(
            BrowserInputs::parse(&raw(
                epoch(12),
                0,
                &"é".repeat(MAX_LOCALE_CHARACTERS + 1),
                None
            )),
            Err(InputError::Locale)
        );
        assert_eq!(
            BrowserInputs::parse(&" ".repeat(MAX_INPUT_BYTES + 1)),
            Err(InputError::TooLarge)
        );
        let mut padded = raw(epoch(12), 0, "en-US", None);
        padded.extend(std::iter::repeat_n(' ', MAX_INPUT_BYTES - padded.len()));
        assert!(BrowserInputs::parse(&padded).is_ok());
        for malformed in [
            "null",
            "{}",
            r#"{"epochMilliseconds":1.5,"timezoneOffsetMinutes":0,"locale":"en-US","storedValue":null}"#,
            r#"{"epochMilliseconds":0,"timezoneOffsetMinutes":0,"locale":"en-US","storedValue":7}"#,
        ] {
            assert_eq!(BrowserInputs::parse(malformed), Err(InputError::Malformed));
        }
    }

    #[test]
    fn browser_inputs_reject_invalid_offsets_and_date_range_without_panicking() {
        for offset in [-1441, -1440, 1440, 1441, i32::MIN, i32::MAX] {
            assert_eq!(
                BrowserInputs::parse(&raw(epoch(12), offset, "en-US", None)),
                Err(InputError::Timezone)
            );
        }
        for offset in [-1439, 0, 1439] {
            assert!(BrowserInputs::parse(&raw(epoch(12), offset, "en-US", None)).is_ok());
        }
        for timestamp in [i64::MIN, i64::MAX] {
            assert_eq!(
                BrowserInputs::parse(&raw(timestamp, 0, "en-US", None)),
                Err(InputError::Date)
            );
        }
    }

    #[test]
    fn greeting_matches_all_original_hour_boundaries_and_icons() {
        for (hour, text, icon) in [
            (0, "Good night", GreetingIcon::Moon),
            (4, "Good night", GreetingIcon::Moon),
            (5, "Good morning", GreetingIcon::Sunrise),
            (11, "Good morning", GreetingIcon::Sunrise),
            (12, "Good afternoon", GreetingIcon::Sun),
            (16, "Good afternoon", GreetingIcon::Sun),
            (17, "Good evening", GreetingIcon::Sunset),
            (20, "Good evening", GreetingIcon::Sunset),
            (21, "Good night", GreetingIcon::Moon),
            (23, "Good night", GreetingIcon::Moon),
        ] {
            let greeting = local_greeting(&inputs(hour, 0, None), None);
            assert_eq!(greeting.text, text, "hour {hour}");
            assert_eq!(greeting.icon, icon, "hour {hour}");
            assert_eq!(greeting.date_label, "Friday, October 2");
        }
    }

    #[test]
    fn browser_offset_controls_local_day_and_name_uses_master_truthiness() {
        let mut user = AuthUser {
            id: 7,
            username: "alice".into(),
            display_name: "Alice A".into(),
            is_admin: false,
        };
        let west = local_greeting(&inputs(1, 420, None), Some(&user));
        assert_eq!(west.text, "Good evening, Alice A");
        assert_eq!(west.icon, GreetingIcon::Sunset);
        assert_eq!(west.date_label, "Thursday, October 1");
        let east = local_greeting(&inputs(23, -120, None), None);
        assert_eq!(east.text, "Good night");
        assert_eq!(east.date_label, "Saturday, October 3");
        user.display_name.clear();
        assert_eq!(
            local_greeting(&inputs(12, 0, None), Some(&user)).text,
            "Good afternoon, alice"
        );
        user.display_name = " ".into();
        assert_eq!(
            local_greeting(&inputs(12, 0, None), Some(&user)).text,
            "Good afternoon,  "
        );
    }

    #[test]
    fn recent_routes_use_typed_destination_and_reject_browser_navigation_injection() {
        for (kind, route, expected) in [
            (RecentType::Issue, "LIF-42", "/LIF/issues/LIF-42"),
            (RecentType::Page, "42", "/LIF/pages/42"),
            (RecentType::Plan, "42", "/LIF/plans/42"),
        ] {
            assert_eq!(
                recent_route(&recent(kind, route, "LIF", 0)).as_deref(),
                Some(expected)
            );
        }
        for (kind, route, project) in [
            (RecentType::Issue, "OTHER-42", "LIF"),
            (RecentType::Issue, "LIF-0", "LIF"),
            (RecentType::Issue, "LIF-42?admin=1", "LIF"),
            (RecentType::Page, "LIF-DOC-42", "LIF"),
            (RecentType::Page, "0", "LIF"),
            (RecentType::Plan, "../42", "LIF"),
            (RecentType::Plan, "9223372036854775808", "LIF"),
            (RecentType::Plan, "42", "//example.org"),
            (RecentType::Plan, "42", "lif"),
        ] {
            assert!(
                recent_route(&recent(kind, route, project, 0)).is_none(),
                "{project}/{route}"
            );
        }
    }

    #[test]
    fn stored_recents_preserve_list_order_and_skip_invalid_raw_entries() {
        let first = recent(RecentType::Issue, "LIF-2", "LIF", 10);
        let second = recent(RecentType::Page, "3", "LIF", 90);
        let mut invalid_title = serde_json::to_value(&first).unwrap();
        invalid_title["title"] = json!(7);
        let mut invalid_timestamp = serde_json::to_value(&first).unwrap();
        invalid_timestamp["ts"] = json!(1.5);
        let stored = json!([
            first,
            {"type":"other","routeId":"2","identifier":"x","title":"x","project":"LIF","ts":1},
            recent(RecentType::Plan, "../42", "LIF", 1),
            invalid_title,
            invalid_timestamp,
            second,
        ])
        .to_string();
        assert_eq!(
            stored_recents(&inputs(12, 0, Some(&stored))),
            vec![first, second]
        );
        for invalid in [None, Some(""), Some("not json"), Some("{}"), Some("null")] {
            assert!(stored_recents(&inputs(12, 0, invalid)).is_empty());
        }
        let entries: Vec<_> = (1..=20)
            .map(|n| recent(RecentType::Page, &n.to_string(), "LIF", n))
            .collect();
        let stored = serde_json::to_string(&entries).unwrap();
        assert_eq!(
            stored_recents(&inputs(12, 0, Some(&stored))),
            entries[..STORED_RECENTS]
        );
    }

    #[test]
    fn recent_visit_dedupes_type_and_route_id_caps_storage_and_display_without_sorting() {
        let mut entries: Vec<_> = (1..=16)
            .map(|n| recent(RecentType::Page, &n.to_string(), "LIF", n))
            .collect();
        entries.insert(0, recent(RecentType::Plan, "3", "LIF", 999));
        entries.insert(1, recent(RecentType::Page, "3", "OTHER", 888));
        let mut visit = recent(RecentType::Page, "3", "LIF", epoch(12));
        visit.title = "Updated title".into();
        let recorded = record_recent(&entries, visit.clone());
        assert_eq!(recorded.len(), STORED_RECENTS);
        assert_eq!(recorded[0], visit);
        assert_eq!(recorded[1], entries[0]);
        assert_eq!(
            recorded
                .iter()
                .filter(|e| e.kind == RecentType::Page && e.route_id == "3")
                .count(),
            1
        );
        assert_eq!(display_recents(&recorded), recorded[..DISPLAY_RECENTS]);
        let encoded: Value = serde_json::to_value(&recorded).unwrap();
        assert_eq!(encoded[0]["type"], "page");
        assert_eq!(encoded[0]["routeId"], "3");
        assert!(encoded[0].get("kind").is_none());
        assert!(encoded[0].get("route_id").is_none());
    }
}
