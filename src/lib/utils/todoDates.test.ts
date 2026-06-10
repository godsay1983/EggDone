import { describe, expect, it } from "vitest";

import type { Todo } from "$lib/types";
import {
  formatDueLabel,
  getDueTone,
  isDueTodayOrOverdue,
  localDateString,
} from "./todoDates";

function makeTodo(overrides: Partial<Todo> = {}): Todo {
  return {
    id: 1,
    uuid: "00000000-0000-4000-8000-000000000001",
    title: "todo",
    group_uuid: null,
    completed: false,
    pinned: false,
    sort_order: 0,
    created_at: 1,
    updated_at: 1,
    completed_at: null,
    deleted_at: null,
    due_date: null,
    due_at: null,
    reminder_at: null,
    repeat_rule: null,
    repeat_next_due_date: null,
    ...overrides,
  };
}

describe("todo date helpers", () => {
  const now = new Date("2026-06-09T12:00:00+08:00");

  it("formats local calendar dates", () => {
    expect(localDateString(0, now)).toBe("2026-06-09");
    expect(localDateString(1, now)).toBe("2026-06-10");
  });

  it("labels today tomorrow and later dates", () => {
    expect(formatDueLabel(makeTodo({ due_date: "2026-06-09" }), now)).toBe(
      "今天",
    );
    expect(formatDueLabel(makeTodo({ due_date: "2026-06-10" }), now)).toBe(
      "明天",
    );
    expect(formatDueLabel(makeTodo({ due_date: "2026-06-12" }), now)).toBe(
      "06/12",
    );
  });

  it("detects overdue and today tasks", () => {
    expect(getDueTone(makeTodo({ due_date: "2026-06-08" }), now)).toBe(
      "overdue",
    );
    expect(getDueTone(makeTodo({ due_date: "2026-06-09" }), now)).toBe(
      "today",
    );
    expect(getDueTone(makeTodo({ due_date: "2026-06-10" }), now)).toBe("");
    expect(
      isDueTodayOrOverdue(makeTodo({ completed: true, due_date: "2026-06-09" }), now),
    ).toBe(false);
  });
});
