// Pure saved-view configuration helpers retained with the upstream tests.
// The native frontend owns rendering, persistence, and navigation.

import { normalizeAssigneeFilter } from "./assignees";

export type Layout = "list" | "board";
type SortField = "priority" | "age" | "number" | "updated";
type SortDir = "asc" | "desc";
type GroupBy = "status" | "priority" | "module" | "none";
type Density = "compact" | "comfortable";
type LaneBy = "none" | "module" | "priority";

export interface ViewConfig {
  version: 1;
  layout: Layout;
  filterStatus: string;
  filterPriority: string;
  filterLabel: string;
  filterModule: string;
  filterAssignee: string;
  searchQuery: string;
  sortField: SortField;
  sortDir: SortDir;
  groupBy: GroupBy;
  density: Density;
  laneBy: LaneBy;
  hiddenStatuses: string[];
}

const FIELDS_COMPARED: (keyof ViewConfig)[] = [
  "filterStatus",
  "filterPriority",
  "filterLabel",
  "filterModule",
  "filterAssignee",
  "searchQuery",
  "sortField",
  "sortDir",
  "groupBy",
  "density",
  "laneBy",
];

/** Parse a saved view config, tolerating absent or invalid fields. */
export function parseConfig(raw: string): ViewConfig | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!parsed || typeof parsed !== "object") return null;
  const p = parsed as Record<string, unknown>;
  const str = (value: unknown, fallback: string) =>
    typeof value === "string" ? value : fallback;
  return {
    version: 1,
    layout: p.layout === "board" ? "board" : "list",
    filterStatus: str(p.filterStatus, ""),
    filterPriority: str(p.filterPriority, ""),
    filterLabel: str(p.filterLabel, ""),
    filterModule: str(p.filterModule, ""),
    filterAssignee: normalizeAssigneeFilter(p.filterAssignee),
    searchQuery: str(p.searchQuery, ""),
    sortField: (["priority", "age", "number", "updated"] as const).includes(
      p.sortField as SortField,
    )
      ? (p.sortField as SortField)
      : "priority",
    sortDir: p.sortDir === "desc" ? "desc" : "asc",
    groupBy: (["status", "priority", "module", "none"] as const).includes(
      p.groupBy as GroupBy,
    )
      ? (p.groupBy as GroupBy)
      : "status",
    density: p.density === "comfortable" ? "comfortable" : "compact",
    laneBy:
      p.laneBy === "module" || p.laneBy === "priority" ? p.laneBy : "none",
    hiddenStatuses: Array.isArray(p.hiddenStatuses)
      ? p.hiddenStatuses.filter((status): status is string => typeof status === "string")
      : [],
  };
}

/** Whether a saved config differs in layout or any persisted view setting. */
export function configsDiffer(a: ViewConfig, b: ViewConfig): boolean {
  if (a.layout !== b.layout) return true;
  if (FIELDS_COMPARED.some((key) => a[key] !== b[key])) return true;
  const ah = [...a.hiddenStatuses].sort();
  const bh = [...b.hiddenStatuses].sort();
  return ah.length !== bh.length || ah.some((status, index) => status !== bh[index]);
}
