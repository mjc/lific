const { describe, expect, test } = require("./assertions.js");
const {
  createActivityRateCounter,
  isActivityRealtimeEvent,
  parseActivityBaseline,
  selectActivityRate
} = require("./activity-adapter.js");
describe("activity rate", () => {
  test("combines a daily baseline with live time buckets", () => {
    const counter = createActivityRateCounter();
    const start = 3600000;
    counter.seed({ dayCount: 10 }, start);
    counter.record(start);
    counter.record(start + 500);
    expect(counter.counts(start + 500)).toEqual({
      perSecond: 2,
      perMinute: 2,
      perHour: 2,
      perDay: 12
    });
    expect(counter.counts(start + 61000)).toEqual({
      perSecond: 0,
      perMinute: 0,
      perHour: 2,
      perDay: 12
    });
  });
  test("seeding and resetting discard stale live buckets", () => {
    const counter = createActivityRateCounter();
    counter.record(1000);
    counter.seed({ dayCount: 4 }, 1000);
    expect(counter.counts(1000)).toEqual({
      perSecond: 0,
      perMinute: 0,
      perHour: 0,
      perDay: 4
    });
    counter.reset();
    expect(counter.counts(1000).perDay).toBe(0);
  });
  test("expires an initial daily baseline without reconnecting", () => {
    const counter = createActivityRateCounter();
    const start = 1000;
    counter.seed({ dayCount: 4 }, start);
    counter.record(start + 3600000);
    expect(counter.counts(start + 86400000).perDay).toBe(1);
  });
  test("excludes buckets that start before each trailing window", () => {
    const counter = createActivityRateCounter();
    counter.record(0);
    expect(counter.counts(1500).perSecond).toBe(0);
    expect(counter.counts(60500).perMinute).toBe(0);
    expect(counter.counts(3600500).perHour).toBe(0);
    expect(counter.counts(86400500).perDay).toBe(0);
  });
  test("counts only audit-backed realtime events", () => {
    expect(isActivityRealtimeEvent("project.updated")).toBe(true);
    expect(isActivityRealtimeEvent("issue.linked")).toBe(true);
    expect(isActivityRealtimeEvent("projects.reordered")).toBe(false);
    expect(isActivityRealtimeEvent("project_groups.changed")).toBe(false);
    expect(isActivityRealtimeEvent("future.configuration.changed")).toBe(false);
  });
  test("accepts only non-negative safe-integer baselines", () => {
    expect(parseActivityBaseline(4)).toEqual({ dayCount: 4 });
    for (const invalid of [-1, 1.5, Number.MAX_SAFE_INTEGER + 1, 1 / 0, "4", null]) {
      expect(parseActivityBaseline(invalid)).toBeNull();
    }
  });
  test("selects the shortest active rate window", () => {
    expect(selectActivityRate({ perSecond: 2, perMinute: 3, perHour: 4, perDay: 5 })).toEqual({ value: 2, unit: "updates/s" });
    expect(selectActivityRate({ perSecond: 1, perMinute: 3, perHour: 4, perDay: 5 })).toEqual({ value: 3, unit: "updates/min" });
    expect(selectActivityRate({ perSecond: 0, perMinute: 1, perHour: 4, perDay: 5 })).toEqual({ value: 4, unit: "updates/hr" });
    expect(selectActivityRate({ perSecond: 0, perMinute: 1, perHour: 1, perDay: 5 })).toEqual({ value: 5, unit: "updates/day" });
  });
});
