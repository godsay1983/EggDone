import { describe, expect, it } from "vitest";

import type { Todo } from "$lib/types";
import { filterTodos } from "./todoFilters";

function makeTodo(
  id: number,
  title: string,
  completed = false,
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
});
