import { describe, expect, it } from "vitest";
import { fitWindow, nextZoom, normalizeWindowPreferences, WINDOW_PRESETS } from "./windowPreferences";

describe("local window preferences", () => {
  it("preserves the existing window for first launch", () => {
    expect(normalizeWindowPreferences(null)).toEqual(WINDOW_PRESETS.small);
  });
  it("rejects malformed persisted fields", () => {
    expect(normalizeWindowPreferences({ width: NaN, height: "900", zoom: 3 })).toEqual(WINDOW_PRESETS.small);
  });
  it("bounds dimensions and rounds logical pixels", () => {
    expect(normalizeWindowPreferences({ width: 99999, height: -5, zoom: 1.25 })).toEqual({ width: 4096, height: 420, zoom: 1.25 });
    expect(normalizeWindowPreferences({ width: 550.4, height: 720.8 }).height).toBe(721);
  });
  it("keeps enough layout space when zooming in", () => {
    const fit = fitWindow({ ...WINDOW_PRESETS.small, zoom: 1.5 }, { width: 1920, height: 1040 });
    expect([fit.width, fit.height]).toEqual([540, 630]);
  });
  it("fits oversized saved windows to a smaller display", () => {
    const fit = fitWindow({ width: 1600, height: 1200, zoom: 1 }, { width: 960, height: 540 });
    expect([fit.width, fit.height]).toEqual([944, 524]);
  });
  it("caps minimum constraints even on tiny work areas", () => {
    const fit = fitWindow(WINDOW_PRESETS.large, { width: 400, height: 300 });
    expect([fit.width, fit.height]).toEqual([384, 284]);
    expect(fit.minWidth).toBeLessThanOrEqual(fit.maxWidth);
    expect(fit.minHeight).toBeLessThanOrEqual(fit.maxHeight);
  });
  it("retains valid user dimensions", () => {
    const fit = fitWindow({ width: 750, height: 700, zoom: 1.15 }, { width: 1280, height: 900 });
    expect([fit.width, fit.height]).toEqual([750, 700]);
  });
  it("steps and bounds keyboard zoom", () => {
    expect(nextZoom(1, 1)).toBe(1.15);
    expect(nextZoom(1.25, -1)).toBe(1.15);
    expect(nextZoom(1.5, 1)).toBe(1.5);
    expect(nextZoom(1, -1)).toBe(1);
  });
});
