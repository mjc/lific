export type IssueStatusTreatment = {
  status: string;
  label: string;
  symbol: string;
  completed: boolean;
};

const STATUS_TREATMENTS: Record<string, IssueStatusTreatment> = {
  backlog: { status: "backlog", label: "Backlog", symbol: "○", completed: false },
  todo: { status: "todo", label: "To do", symbol: "□", completed: false },
  active: { status: "active", label: "Active", symbol: "◐", completed: false },
  done: { status: "done", label: "Done", symbol: "✓", completed: true },
  cancelled: { status: "cancelled", label: "Cancelled", symbol: "×", completed: true },
};

export function issueStatusTreatment(status: string): IssueStatusTreatment {
  const normalized = status.trim().toLowerCase();
  return STATUS_TREATMENTS[normalized] ?? {
    status: "unknown",
    label: "Unknown status",
    symbol: "?",
    completed: false,
  };
}
