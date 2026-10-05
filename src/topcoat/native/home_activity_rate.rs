//! Native activity-rate behavior from the pinned master activityRate.ts.

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) per_second: u64,
    pub(crate) per_minute: u64,
    pub(crate) per_hour: u64,
    pub(crate) per_day: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Rate {
    pub(crate) value: u64,
    pub(crate) unit: &'static str,
}

const DAY_MS: i64 = 86_400_000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Bucket {
    at: i64,
    count: u64,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Counter {
    seconds: std::collections::VecDeque<Bucket>,
    minutes: std::collections::VecDeque<Bucket>,
    baseline: u64,
    baseline_at: Option<i64>,
}

impl Counter {
    pub(crate) fn seed(&mut self, day_count: u64, now: i64) {
        self.reset();
        self.baseline = day_count;
        self.baseline_at = Some(now);
    }
    pub(crate) fn record(&mut self, now: i64) {
        self.prune(now);
        for (buckets, width) in [(&mut self.seconds, 1_000), (&mut self.minutes, 60_000)] {
            let at = now.div_euclid(width) * width;
            if let Some(last) = buckets.back_mut().filter(|last| last.at == at) {
                last.count = last.count.saturating_add(1);
            } else {
                buckets.push_back(Bucket { at, count: 1 });
            }
        }
    }
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
    fn prune(&mut self, now: i64) {
        for (buckets, window) in [(&mut self.seconds, 60_000), (&mut self.minutes, DAY_MS)] {
            let cutoff = now.saturating_sub(window);
            while buckets.front().is_some_and(|bucket| bucket.at < cutoff) {
                buckets.pop_front();
            }
        }
    }
    pub(crate) fn counts(&mut self, now: i64) -> Counts {
        self.prune(now);
        let sum = |buckets: &std::collections::VecDeque<Bucket>, window| {
            buckets
                .iter()
                .filter(|bucket| bucket.at >= now.saturating_sub(window))
                .fold(0_u64, |total, bucket| total.saturating_add(bucket.count))
        };
        let baseline = self
            .baseline_at
            .filter(|at| now.saturating_sub(*at) < DAY_MS)
            .map_or(0, |_| self.baseline);
        Counts {
            per_second: sum(&self.seconds, 1_000),
            per_minute: sum(&self.seconds, 60_000),
            per_hour: sum(&self.minutes, 3_600_000),
            per_day: baseline.saturating_add(sum(&self.minutes, DAY_MS)),
        }
    }
}

// Conversion is admitted only after finite, nonnegative, integral range checks.
#[allow(clippy::cast_sign_loss)]
fn baseline_number(number: f64) -> Option<u64> {
    if number.is_finite()
        && number >= 0.0
        && number.fract() == 0.0
        && number <= MAX_SAFE_INTEGER as f64
    {
        Some(number as u64)
    } else {
        None
    }
}
#[cfg(test)]
pub(crate) fn parse_baseline(value: &serde_json::Value) -> Option<u64> {
    baseline_number(value.as_f64()?)
}
#[cfg(test)]
pub(crate) fn counted_event_type(kind: &str) -> bool {
    serde_json::from_value::<crate::realtime::RealtimeEvent>(
        serde_json::json!({"type":kind,"project_id":1,"issue_id":1}),
    )
    .is_ok_and(|event| counted_event(&event))
}
pub(crate) fn counted_event(event: &crate::realtime::RealtimeEvent) -> bool {
    use crate::realtime::RealtimeEvent;
    matches!(
        event,
        RealtimeEvent::ProjectCreated { .. }
            | RealtimeEvent::ProjectUpdated { .. }
            | RealtimeEvent::ProjectDeleted { .. }
            | RealtimeEvent::IssueCreated { .. }
            | RealtimeEvent::IssueUpdated { .. }
            | RealtimeEvent::IssueDeleted { .. }
            | RealtimeEvent::IssueLinked { .. }
            | RealtimeEvent::IssueUnlinked { .. }
    )
}
pub(crate) fn select_rate(counts: Counts) -> Rate {
    for (value, unit) in [
        (counts.per_second, "updates/s"),
        (counts.per_minute, "updates/min"),
        (counts.per_hour, "updates/hr"),
    ] {
        if value >= 2 {
            return Rate { value, unit };
        }
    }
    Rate {
        value: counts.per_day,
        unit: "updates/day",
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct State {
    account: i64,
    admin: bool,
    #[serde(default)]
    connection_epoch: Option<String>,
    pub(crate) counter: Counter,
}

impl State {
    pub(crate) fn admit_connection(&mut self, epoch: &str) {
        if self.connection_epoch.as_deref() != Some(epoch) {
            self.counter.reset();
            self.connection_epoch = Some(epoch.to_owned());
        }
    }
    pub(crate) fn baseline_due(&self, now: i64) -> bool {
        self.counter
            .baseline_at
            .is_none_or(|at| now.saturating_sub(at) >= 3_600_000)
    }
    pub(crate) fn presentation(&mut self, now: i64) -> (bool, Rate) {
        (self.ready(), select_rate(self.counter.counts(now)))
    }
    pub(crate) fn record(&mut self, now: i64) {
        if self.ready() {
            self.counter.record(now);
        }
    }
    pub(crate) fn account(&self) -> i64 {
        self.account
    }
    pub(crate) fn admin(&self) -> bool {
        self.admin
    }
    pub(crate) fn restore(raw: &str, account: i64, admin: bool, now: i64) -> Self {
        let fresh = || Self {
            account,
            admin,
            connection_epoch: None,
            counter: Counter::default(),
        };
        if raw.len() > 128 * 1024 {
            return fresh();
        }
        let Ok(mut restored) = serde_json::from_str::<Self>(raw) else {
            return fresh();
        };
        let valid_buckets = |buckets: &std::collections::VecDeque<Bucket>, width, maximum| {
            buckets.len() <= maximum
                && buckets.iter().all(|bucket| {
                    bucket.at <= now
                        && bucket.at.rem_euclid(width) == 0
                        && bucket.count > 0
                        && bucket.count <= MAX_SAFE_INTEGER
                })
                && buckets
                    .iter()
                    .zip(buckets.iter().skip(1))
                    .all(|(a, b)| a.at < b.at)
        };
        if restored.account != account
            || restored.admin != admin
            || restored.counter.baseline > MAX_SAFE_INTEGER
            || restored
                .connection_epoch
                .as_ref()
                .is_some_and(|epoch| epoch.len() > 128)
            || restored.counter.baseline_at.is_some_and(|at| at > now)
            || !valid_buckets(&restored.counter.seconds, 1_000, 61)
            || !valid_buckets(&restored.counter.minutes, 60_000, 1_441)
        {
            return fresh();
        }
        restored.counter.prune(now);
        restored
    }

    pub(crate) fn ready(&self) -> bool {
        self.counter.baseline_at.is_some()
    }

    pub(crate) fn seed(
        &mut self,
        cx: &topcoat::context::Cx,
        user: &crate::db::models::AuthUser,
        now: i64,
    ) -> bool {
        match crate::realtime::activity_baseline(super::context::db(cx), user) {
            Ok(crate::realtime::RealtimeEvent::ActivityBaseline { day_count }) => {
                match baseline_number(day_count as f64) {
                    Some(count) => {
                        self.counter.seed(count, now);
                        true
                    }
                    None => false,
                }
            }
            Ok(_) => false,
            Err(error) => {
                tracing::warn!(error = %error, "native activity baseline unavailable");
                false
            }
        }
    }

    pub(crate) fn render<'a>(
        &mut self,
        cx: &'a topcoat::context::Cx,
        state: &topcoat::runtime::Signal<String>,
        now: i64,
    ) -> topcoat::view::BoxView<'a> {
        use topcoat::{
            runtime::{Event, expr},
            view::{ViewExt, view},
        };
        let rate = select_rate(self.counter.counts(now));
        let ready = self.ready();
        let stored = state.clone();
        let encoded = serde_json::to_string(self).expect("activity state is serializable");
        let persist = expr!(|_event: Event| {
            stored.set(encoded.clone());
        });
        view! { cx =>
            <span data-native-home-activity-rate="" hidden=(!ready) @mount=(persist)
                title="Websocket activity rate; the day fallback includes the last 24 hours">
                (rate.value.to_string()) " " (rate.unit)
            </span>
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_activity_rate_lifecycle_baseline_is_due_immediately_and_hourly() {
        let mut state = State::restore("", 7, false, 0);
        state.admit_connection("first-physical-socket");
        assert!(state.baseline_due(0));
        state.counter.seed(4, 0);
        assert!(!state.baseline_due(3_599_999));
        assert!(state.baseline_due(3_600_000));
        state.counter.seed(7, 3_600_000);
        assert!(!state.baseline_due(7_199_999));
        assert!(state.baseline_due(7_200_000));
    }

    #[test]
    fn native_activity_rate_lifecycle_new_socket_resets_but_content_rerender_keeps_counter() {
        let mut state = State::restore("", 7, false, 1_000);
        state.admit_connection("first-physical-socket");
        state.counter.seed(4, 1_000);
        state.record(1_000);
        let encoded = serde_json::to_string(&state).unwrap();
        let mut rerender = State::restore(&encoded, 7, false, 1_500);
        rerender.admit_connection("first-physical-socket");
        assert!(rerender.ready());
        assert_eq!(rerender.counter.counts(1_500).per_day, 5);
        assert!(!rerender.baseline_due(1_500));
        rerender.admit_connection("replacement-physical-socket");
        assert!(!rerender.ready());
        assert_eq!(rerender.counter.counts(1_500), Counts::default());
        assert!(rerender.baseline_due(1_500));
    }

    #[test]
    fn native_activity_rate_lifecycle_zero_baseline_changes_readiness_without_changing_rate() {
        let mut state = State::restore("", 7, false, 0);
        let before = state.presentation(0);
        state.counter.seed(0, 0);
        let after = state.presentation(0);
        assert_eq!(before.1, after.1);
        assert!(!before.0);
        assert!(after.0);
        assert_ne!(before, after);
        let restored = State::restore(&serde_json::to_string(&state).unwrap(), 7, false, 1_000);
        assert!(restored.ready());
    }

    #[test]
    fn native_activity_rate_lifecycle_records_audited_events_only_after_baseline_ready() {
        let mut state = State::restore("", 7, false, 0);
        state.record(0);
        assert_eq!(state.counter.counts(0), Counts::default());
        state.counter.seed(0, 0);
        state.record(0);
        assert_eq!(state.counter.counts(0).per_day, 1);
    }

    #[test]
    fn native_activity_rate_combines_daily_baseline_with_live_time_buckets() {
        let mut counter = Counter::default();
        let start = 3_600_000;
        counter.seed(10, start);
        counter.record(start);
        counter.record(start + 500);
        assert_eq!(
            counter.counts(start + 500),
            Counts {
                per_second: 2,
                per_minute: 2,
                per_hour: 2,
                per_day: 12
            }
        );
        assert_eq!(
            counter.counts(start + 61_000),
            Counts {
                per_second: 0,
                per_minute: 0,
                per_hour: 2,
                per_day: 12
            }
        );
    }

    #[test]
    fn native_activity_rate_seeding_and_reset_discard_stale_live_buckets() {
        let mut counter = Counter::default();
        counter.record(1_000);
        counter.seed(4, 1_000);
        assert_eq!(
            counter.counts(1_000),
            Counts {
                per_day: 4,
                ..Counts::default()
            }
        );
        counter.reset();
        assert_eq!(counter.counts(1_000).per_day, 0);
    }

    #[test]
    fn native_activity_rate_expires_initial_daily_baseline_without_reconnect() {
        let mut counter = Counter::default();
        let start = 1_000;
        counter.seed(4, start);
        counter.record(start + 3_600_000);
        assert_eq!(counter.counts(start + 86_400_000).per_day, 1);
    }

    #[test]
    fn native_activity_rate_excludes_buckets_before_each_trailing_window() {
        let mut counter = Counter::default();
        counter.record(0);
        assert_eq!(counter.counts(1_500).per_second, 0);
        assert_eq!(counter.counts(60_500).per_minute, 0);
        assert_eq!(counter.counts(3_600_500).per_hour, 0);
        assert_eq!(counter.counts(86_400_500).per_day, 0);
        // The original groups events into second/minute bucket starts.
        counter.record(61_999);
        assert_eq!(counter.counts(63_000).per_second, 0);
        assert_eq!(counter.counts(3_660_001).per_hour, 0);
    }

    #[test]
    fn native_activity_rate_counts_only_audit_backed_realtime_events() {
        for kind in [
            "project.created",
            "project.updated",
            "project.deleted",
            "issue.created",
            "issue.updated",
            "issue.deleted",
            "issue.linked",
            "issue.unlinked",
        ] {
            assert!(counted_event_type(kind), "{kind}");
        }
        for kind in [
            "projects.reordered",
            "project_groups.changed",
            "future.configuration.changed",
            "page.updated",
            "comment.created",
            "resync.required",
        ] {
            assert!(!counted_event_type(kind), "{kind}");
        }
    }

    #[test]
    fn native_activity_rate_accepts_only_nonnegative_safe_integer_baselines() {
        assert_eq!(parse_baseline(&json!(4)), Some(4));
        assert_eq!(parse_baseline(&json!(4.0)), Some(4));
        assert_eq!(
            parse_baseline(&json!(9_007_199_254_740_991_u64)),
            Some(9_007_199_254_740_991)
        );
        for invalid in [
            json!(-1),
            json!(1.5),
            json!(9_007_199_254_740_992_u64),
            json!("4"),
            json!(null),
            json!(true),
        ] {
            assert_eq!(parse_baseline(&invalid), None, "{invalid}");
        }
    }

    #[test]
    fn native_activity_rate_selects_shortest_active_window() {
        for (counts, expected) in [
            (
                Counts {
                    per_second: 2,
                    per_minute: 3,
                    per_hour: 4,
                    per_day: 5,
                },
                Rate {
                    value: 2,
                    unit: "updates/s",
                },
            ),
            (
                Counts {
                    per_second: 1,
                    per_minute: 3,
                    per_hour: 4,
                    per_day: 5,
                },
                Rate {
                    value: 3,
                    unit: "updates/min",
                },
            ),
            (
                Counts {
                    per_second: 0,
                    per_minute: 1,
                    per_hour: 4,
                    per_day: 5,
                },
                Rate {
                    value: 4,
                    unit: "updates/hr",
                },
            ),
            (
                Counts {
                    per_second: 0,
                    per_minute: 1,
                    per_hour: 1,
                    per_day: 5,
                },
                Rate {
                    value: 5,
                    unit: "updates/day",
                },
            ),
        ] {
            assert_eq!(select_rate(counts), expected);
        }
    }

    #[test]
    fn native_activity_rate_restores_same_owner_and_rejects_foreign_malformed_future_state() {
        let mut state = State::restore("", 7, false, 61_000);
        state.counter.seed(4, 61_000);
        state.counter.record(61_000);
        let encoded = serde_json::to_string(&state).unwrap();
        let mut restored = State::restore(&encoded, 7, false, 61_500);
        assert!(restored.ready());
        assert_eq!(restored.counter.counts(61_500).per_day, 5);
        for (account, admin, now) in [(8, false, 61_500), (7, true, 61_500), (7, false, 60_999)] {
            assert!(!State::restore(&encoded, account, admin, now).ready());
        }
        for invalid in ["null", "{}", "not json"] {
            assert!(!State::restore(invalid, 7, false, 61_500).ready());
        }
        let mut hostile = serde_json::to_value(&state).unwrap();
        hostile["counter"]["seconds"] = json!([{"at":61001,"count":1}]);
        assert!(!State::restore(&hostile.to_string(), 7, false, 62_000).ready());
        hostile["counter"]["seconds"] = json!([{"at":61000,"count":1},{"at":61000,"count":1}]);
        assert!(!State::restore(&hostile.to_string(), 7, false, 62_000).ready());
        hostile["counter"]["seconds"] = json!(vec![json!({"at":61000,"count":1}); 62]);
        assert!(!State::restore(&hostile.to_string(), 7, false, 62_000).ready());
        assert!(!State::restore(&" ".repeat(128 * 1024 + 1), 7, false, 62_000).ready());
    }

    #[topcoat::view::component]
    async fn rate_fixture(
        cx: &topcoat::context::Cx,
        ready: bool,
    ) -> topcoat::Result<impl topcoat::view::View> {
        let stored = topcoat::runtime::signal(cx, String::new);
        let mut state = State::restore("", 7, false, 0);
        if ready {
            state.counter.seed(12, 0);
        }
        Ok(state.render(cx, &stored, 0))
    }

    #[tokio::test]
    async fn native_activity_rate_view_keeps_exact_original_ready_label_and_title() {
        use topcoat::view::{ViewExt, view};
        let cx = topcoat::context::Cx::default();
        let html = view! { cx => rate_fixture(ready: true) }
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(html.contains("12 updates/day"));
        assert!(html.contains(
            "title=\"Websocket activity rate; the day fallback includes the last 24 hours\""
        ));
        assert!(html.contains("data-topcoat-on:mount"));
        assert!(!html.contains("hidden=\"hidden\""));
        let unavailable = view! { cx => rate_fixture(ready: false) }
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(unavailable.contains("hidden"));
    }
}
