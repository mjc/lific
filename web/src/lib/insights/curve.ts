// LIF-240 — smooth SVG path helpers for the Insights trend chart. No chart
// library: a Catmull-Rom-to-Bezier conversion is enough to turn a handful
// of weekly data points into a pleasant curve without external deps.

export interface Point {
  x: number;
  y: number;
}

/** Smooth "M ... C ..." path through every point via Catmull-Rom splines
 *  converted to cubic Beziers (endpoints clamped by repeating the first/
 *  last point). Degrades to a single "M" for 0-1 points. */
export function smoothPath(points: Point[]): string {
  if (points.length === 0) return "";
  if (points.length === 1) return `M ${points[0].x} ${points[0].y}`;

  let d = `M ${points[0].x} ${points[0].y}`;
  for (let i = 0; i < points.length - 1; i++) {
    const p0 = points[i === 0 ? 0 : i - 1];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = points[i + 2 < points.length ? i + 2 : points.length - 1];
    const cp1x = p1.x + (p2.x - p0.x) / 6;
    const cp1y = p1.y + (p2.y - p0.y) / 6;
    const cp2x = p2.x - (p3.x - p1.x) / 6;
    const cp2y = p2.y - (p3.y - p1.y) / 6;
    d += ` C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${p2.x} ${p2.y}`;
  }
  return d;
}

/** Same smooth line, closed down to `baselineY` to form a fillable area. */
export function smoothAreaPath(points: Point[], baselineY: number): string {
  if (points.length === 0) return "";
  if (points.length === 1) {
    const p = points[0];
    return `M ${p.x} ${baselineY} L ${p.x} ${p.y} L ${p.x} ${baselineY} Z`;
  }
  const line = smoothPath(points);
  const first = points[0];
  const last = points[points.length - 1];
  return `${line} L ${last.x} ${baselineY} L ${first.x} ${baselineY} Z`;
}

/** Round `v` up to a "nice" axis maximum (1/2/5/10 × 10^n), so gridlines
 *  land on friendly numbers instead of the raw data max. */
export function niceMax(v: number): number {
  if (v <= 0) return 4;
  const exp = Math.floor(Math.log10(v));
  const base = 10 ** exp;
  const norm = v / base;
  const niceNorm = norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10;
  return niceNorm * base;
}

/** Evenly spaced integer gridline values from 0 to a nice-rounded max. */
export function niceTicks(max: number): number[] {
  const m = niceMax(max);
  if (m <= 5) return Array.from({ length: m + 1 }, (_, i) => i);
  const step = m / 4;
  return [0, 1, 2, 3, 4].map((i) => Math.round(step * i));
}
