import { describe, expect, it } from "vitest";
import { matchesSmartView, normalizeSmartView, SMART_VIEW_IDS, type SmartViewTask } from "./smartViews";

function task(overrides: Partial<SmartViewTask> = {}): SmartViewTask {
  return { completed: false, completedAt: null, deletedAt: null, archivedAt: null,
    dueAt: null, dueDate: null, priority: 0, ...overrides };
}

describe("smart views", () => {
  const now = new Date(2026, 8, 5, 12).getTime();
  it("only restores supported local IDs", () => {
    for (const id of SMART_VIEW_IDS) expect(normalizeSmartView(id)).toBe(id);
    for (const id of [null, "", "today", "OVERDUE"]) expect(normalizeSmartView(id)).toBeNull();
  });
  it("uses calendar days, not the current time, for overdue", () => {
    expect(matchesSmartView(task({ dueAt: new Date(2026, 8, 5, 8).getTime() }), "overdue", now)).toBe(false);
    expect(matchesSmartView(task({ dueDate: "2026-09-04" }), "overdue", now)).toBe(true);
    expect(matchesSmartView(task({ dueDate: "2026-09-05" }), "overdue", new Date(2026, 8, 6).getTime())).toBe(true);
  });
  it("includes exactly today and the next six local days", () => {
    expect(matchesSmartView(task({ dueDate: "2026-09-05" }), "next7", now)).toBe(true);
    expect(matchesSmartView(task({ dueAt: new Date(2026, 8, 11, 23, 59, 59, 999).getTime() }), "next7", now)).toBe(true);
    expect(matchesSmartView(task({ dueDate: "2026-09-12" }), "next7", now)).toBe(false);
    expect(matchesSmartView(task({ dueDate: "2026-02-30" }), "next7", now)).toBe(false);
  });
  it("requires an actual completion within the last seven calendar days", () => {
    const start = new Date(2026, 7, 30).getTime();
    expect(matchesSmartView(task({ completed: true, completedAt: start }), "recently_completed", now)).toBe(true);
    for (const at of [null, start - 1, now + 1]) {
      expect(matchesSmartView(task({ completed: true, completedAt: at }), "recently_completed", now)).toBe(false);
    }
    expect(matchesSmartView(task({ completedAt: now }), "recently_completed", now)).toBe(false);
  });
  it("excludes deleted, archived and completed tasks from active lists", () => {
    for (const overrides of [{ deletedAt: now }, { archivedAt: now }, { completed: true }]) {
      for (const id of ["overdue", "next7", "no_date", "important"] as const) {
        expect(matchesSmartView(task({ priority: 1, ...overrides }), id, now)).toBe(false);
      }
    }
  });
  it("filters 500 records without mutating records or ordering", () => {
    const items = Array.from({ length: 500 }, (_, i) => Object.freeze(task({ priority: i % 2 })));
    const original = JSON.stringify(items);
    for (let i = 0; i < 10; i++) {
      expect(items.filter((item) => matchesSmartView(item, "important", now))).toHaveLength(250);
    }
    expect(JSON.stringify(items)).toBe(original);
  });
});
