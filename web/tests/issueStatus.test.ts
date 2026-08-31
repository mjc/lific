import { describe, expect, test } from "bun:test";
import { issueStatusTreatment } from "../src/lib/issueStatus";

describe("issueStatusTreatment", () => {
  test("gives every issue status a visible, non-color treatment", () => {
    expect(issueStatusTreatment("backlog")).toEqual({
      status: "backlog",
      label: "Backlog",
      symbol: "○",
      completed: false,
    });
    expect(issueStatusTreatment("todo")).toEqual({
      status: "todo",
      label: "To do",
      symbol: "□",
      completed: false,
    });
    expect(issueStatusTreatment("active")).toEqual({
      status: "active",
      label: "Active",
      symbol: "◐",
      completed: false,
    });
    expect(issueStatusTreatment("done")).toEqual({
      status: "done",
      label: "Done",
      symbol: "✓",
      completed: true,
    });
    expect(issueStatusTreatment("cancelled")).toEqual({
      status: "cancelled",
      label: "Cancelled",
      symbol: "×",
      completed: true,
    });
  });

  test("normalizes unknown or mixed-case statuses to an explicit fallback", () => {
    expect(issueStatusTreatment(" ACTIVE ")).toEqual({
      status: "active",
      label: "Active",
      symbol: "◐",
      completed: false,
    });
    expect(issueStatusTreatment("archived")).toEqual({
      status: "unknown",
      label: "Unknown status",
      symbol: "?",
      completed: false,
    });
  });
});
