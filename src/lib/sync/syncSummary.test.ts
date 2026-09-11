import { describe, expect, it } from "vitest";
import cases from "../../../docs/fixtures/sync-summary-v1.json";
import { syncSummaryState, syncSummaryTone } from "./syncSummary";
import { zhCN } from "../i18n/locales/zh-CN";
import { enUS } from "../i18n/locales/en-US";

describe("shared sync summary", () => {
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
