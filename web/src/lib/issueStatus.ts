export type IssueStatusTreatment = {
  status: string;
  label: string;
  completed: boolean;
};

const STATUS_TREATMENTS: Record<string, IssueStatusTreatment> = {
  backlog: { status: "backlog", label: "Backlog", completed: false },
  todo: { status: "todo", label: "To do", completed: false },
  active: { status: "active", label: "Active", completed: false },
  done: { status: "done", label: "Done", completed: true },
  cancelled: { status: "cancelled", label: "Cancelled", completed: true },
};

export function issueStatusTreatment(status: string): IssueStatusTreatment {
  const normalized = status.trim().toLowerCase();
  return STATUS_TREATMENTS[normalized] ?? {
    status: "unknown",
    label: "Unknown status",
    completed: false,
  };
}
