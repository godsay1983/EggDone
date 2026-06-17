import { describe, expect, it } from "vitest";

import {
  initialListView,
  normalizeDefaultListViewMode,
  normalizeListView,
} from "./viewPreferences";

describe("view preferences", () => {
  it("normalizes default view modes", () => {
    expect(normalizeDefaultListViewMode("all")).toBe("all");
    expect(normalizeDefaultListViewMode("today")).toBe("today");
    expect(normalizeDefaultListViewMode("quadrants")).toBe("quadrants");
    expect(normalizeDefaultListViewMode("remember")).toBe("remember");
    expect(normalizeDefaultListViewMode("bad")).toBe("remember");
    expect(normalizeDefaultListViewMode(null)).toBe("remember");
  });

  it("normalizes list views", () => {
    expect(normalizeListView("today")).toBe("today");
    expect(normalizeListView("quadrants")).toBe("quadrants");
    expect(normalizeListView("all")).toBe("all");
    expect(normalizeListView("bad")).toBe("all");
    expect(normalizeListView(null)).toBe("all");
  });

  it("resolves the startup view from the configured mode", () => {
    expect(initialListView("remember", "today")).toBe("today");
    expect(initialListView("remember", "quadrants")).toBe("quadrants");
    expect(initialListView("remember", "all")).toBe("all");
    expect(initialListView("today", "all")).toBe("today");
    expect(initialListView("quadrants", "all")).toBe("quadrants");
    expect(initialListView("all", "today")).toBe("all");
  });
});
