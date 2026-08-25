import { describe, expect, it } from "vitest";

import {
  fontScaleOptions,
  normalizeFontScale,
  type FontScaleLevel,
} from "./fontScale";

describe("font scale preferences", () => {
  it("covers all levels with a scale factor", () => {
    expect(fontScaleOptions.map((option) => option.value)).toEqual([
      "standard",
      "large",
      "xlarge",
    ]);
    expect(fontScaleOptions.every((option) => option.scale >= 1)).toBe(true);
  });

  it("normalizes unknown values to standard", () => {
    expect(normalizeFontScale("standard")).toBe("standard");
    expect(normalizeFontScale("large")).toBe("large");
    expect(normalizeFontScale("xlarge")).toBe("xlarge");
    expect(normalizeFontScale("huge")).toBe("standard");
    expect(normalizeFontScale(null)).toBe("standard");
  });

  it("keeps every option's label as a settings translation key", () => {
    const labels = fontScaleOptions.map((option) => option.label);
    expect(labels.every((label) => label.startsWith("settings.fontSize"))).toBe(
      true,
    );
  });

  it("exposes a scale for every valid level", () => {
    const levels: FontScaleLevel[] = ["standard", "large", "xlarge"];
    for (const level of levels) {
      const option = fontScaleOptions.find((item) => item.value === level);
      expect(option).toBeDefined();
    }
  });
});
