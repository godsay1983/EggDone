import { describe, expect, it } from "vitest";

import type { Todo } from "$lib/types";
import { filterTodos } from "./todoFilters";

function makeTodo(
  id: number,
  title: string,
  completed = false,
  overrides: Partial<Todo> = {},
): Todo {
  return {
    id,
    uuid: `00000000-0000-4000-8000-${id.toString().padStart(12, "0")}`,
    title,
    completed,
    pinned: false,
    sort_order: id * 1024,
    created_at: id,
    updated_at: id,
    completed_at: completed ? id : null,
    deleted_at: null,
    due_date: null,
    due_at: null,
    reminder_at: null,
    ...overrides,
  };
}

describe("filterTodos", () => {
  const items = [
    makeTodo(1, "Write release notes"),
    makeTodo(2, "购买鸡蛋"),
    makeTodo(3, "WRITE tests", true),
  ];

  it("matches titles without changing the original order", () => {
    expect(filterTodos(items, "write", true).map((todo) => todo.id)).toEqual([
      1, 3,
    ]);
    expect(items.map((todo) => todo.id)).toEqual([1, 2, 3]);
  });

  it("trims the query and supports Chinese text", () => {
    expect(filterTodos(items, "  鸡蛋  ", true).map((todo) => todo.id)).toEqual([
      2,
    ]);
  });

  it("hides completed tasks before applying the search", () => {
    expect(filterTodos(items, "write", false).map((todo) => todo.id)).toEqual([
      1,
    ]);
    expect(filterTodos(items, "", false).map((todo) => todo.id)).toEqual([
      1, 2,
    ]);
  });

  it("shows only incomplete todos due today or overdue in today view", () => {
    const scheduled = [
      makeTodo(1, "overdue", false, { due_date: "2026-06-08" }),
      makeTodo(2, "today", false, { due_date: "2026-06-09" }),
      makeTodo(3, "future", false, { due_date: "2026-06-10" }),
      makeTodo(4, "done today", true, {
        completed_at: 1,
        due_date: "2026-06-09",
      }),
      makeTodo(5, "unscheduled"),
    ];

    expect(
      filterTodos(scheduled, "", true, {
        view: "today",
        now: new Date("2026-06-09T12:00:00+08:00"),
      }).map((todo) => todo.id),
    ).toEqual([1, 2]);
  });

  it("combines today view with title search", () => {
    const scheduled = [
      makeTodo(1, "Write report", false, { due_date: "2026-06-09" }),
      makeTodo(2, "Buy eggs", false, { due_date: "2026-06-09" }),
    ];

    expect(
      filterTodos(scheduled, "write", true, {
        view: "today",
        now: new Date("2026-06-09T12:00:00+08:00"),
      }).map((todo) => todo.id),
    ).toEqual([1]);
  });
});
