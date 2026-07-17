export type ActivityCounts = {
  perSecond: number;
  perMinute: number;
  perHour: number;
  perDay: number;
};

export type ActivityCountsReader = (now: number) => ActivityCounts;

type ActivityBucket = {
  at: number;
  count: number;
};

export type ActivityBaseline = {
  dayCount: number;
};

export type ActivityRate = {
  value: number;
  unit: "updates/s" | "updates/min" | "updates/hr" | "updates/day";
};

export function createActivityRateCounter() {
  const secondBuckets: ActivityBucket[] = [];
  const minuteBuckets: ActivityBucket[] = [];
  let baselineDayCount = 0;

  function prune(now: number) {
    const minuteAgo = now - 60_000;
    while (secondBuckets.length > 0 && secondBuckets[0].at + 1_000 <= minuteAgo) {
      secondBuckets.shift();
    }
    const dayAgo = now - 86_400_000;
    while (minuteBuckets.length > 0 && minuteBuckets[0].at + 60_000 <= dayAgo) {
      minuteBuckets.shift();
    }
  }

  function addBucket(buckets: ActivityBucket[], at: number, count = 1) {
    const bucket = buckets.at(-1);
    if (bucket?.at === at) {
      bucket.count += count;
    } else {
      buckets.push({ at, count });
    }
  }

  function record(now: number) {
    prune(now);
    addBucket(secondBuckets, Math.floor(now / 1_000) * 1_000);
    addBucket(minuteBuckets, Math.floor(now / 60_000) * 60_000);
  }

  function seed(baseline: ActivityBaseline) {
    secondBuckets.length = 0;
    minuteBuckets.length = 0;
    baselineDayCount = baseline.dayCount;
  }

  function sumSince(buckets: ActivityBucket[], now: number, window: number, bucketSize: number) {
    const cutoff = now - window;
    return buckets.reduce(
      (total, bucket) => total + (bucket.at + bucketSize > cutoff ? bucket.count : 0),
      0,
    );
  }

  function counts(now: number): ActivityCounts {
    prune(now);
    return {
      perSecond: sumSince(secondBuckets, now, 1_000, 1_000),
      perMinute: sumSince(secondBuckets, now, 60_000, 1_000),
      perHour: sumSince(minuteBuckets, now, 3_600_000, 60_000),
      perDay: baselineDayCount + minuteBuckets.reduce((total, bucket) => total + bucket.count, 0),
    };
  }

  function reset() {
    secondBuckets.length = 0;
    minuteBuckets.length = 0;
    baselineDayCount = 0;
  }

  return { record, seed, counts, reset };
}

export function selectActivityRate(counts: ActivityCounts): ActivityRate {
  // Two or more events select the shortest meaningful window; otherwise the
  // trailing day remains the conservative fallback.
  if (counts.perSecond >= 2) return { value: counts.perSecond, unit: "updates/s" };
  if (counts.perMinute >= 2) return { value: counts.perMinute, unit: "updates/min" };
  if (counts.perHour >= 2) return { value: counts.perHour, unit: "updates/hr" };
  return { value: counts.perDay, unit: "updates/day" };
}
