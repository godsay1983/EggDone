import { describe, expect, it } from "vitest";
import cases from "../../../docs/fixtures/sync-summary-v1.json";
import { syncSummaryState, syncSummaryTone } from "./syncSummary";
import { zhCN } from "../i18n/locales/zh-CN";
import { enUS } from "../i18n/locales/en-US";

describe("shared sync summary", () => {
  it("shows incomplete cleanup as a warning, not failure or full success", () => {
    const state = syncSummaryState({ enabled: true, configured: true, busy: false, kind: "cleanup_pending", dirty: false, pendingUploads: 0, failedUploads: 0 });
    expect(state).toBe("cleanup_pending");
    expect(syncSummaryTone(state)).toBe("warning");
    expect(zhCN[`sync.summary.${state}`]).toBeTruthy();
    expect(enUS[`sync.explain.${state}`]).toBeTruthy();
  });
  it.each(cases)("$id", ({ input, state, tone }) => {
    const before = JSON.stringify(input);
    const actual = syncSummaryState(input);
    expect(actual).toBe(state);
    expect(syncSummaryTone(actual)).toBe(tone);
    expect(JSON.stringify(input)).toBe(before);
    expect(zhCN[`sync.summary.${actual}`]).toBeTruthy();
    expect(enUS[`sync.explain.${actual}`]).toBeTruthy();
  });
});
