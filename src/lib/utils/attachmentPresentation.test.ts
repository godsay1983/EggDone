import { describe, expect, it } from "vitest";
import fixtures from "../../../docs/fixtures/attachment-presentation-v1.json";
import { attachmentStatus, attachmentRetry, attachmentFailureHint } from "./attachmentPresentation";
import { zhCN } from "../i18n/locales/zh-CN";
import { enUS } from "../i18n/locales/en-US";
const locales: Readonly<Record<string, unknown>>[] = [zhCN, enUS];

describe("shared attachment presentation", () => {
  it.each(fixtures.cases)("$id", ({ input, status, retry }) => {
    const before = JSON.stringify(input);
    expect(attachmentStatus(input)).toBe(status);
    expect(attachmentRetry(input)).toBe(retry);
    expect(JSON.stringify(input)).toBe(before);
    for (const locale of locales) {
      expect(locale[`attachment.state.${status}`]).toBeTruthy();
      expect(locale[`attachment.state.retry_${retry === "upload" ? "upload" : "download"}`]).toBeTruthy();
      if (!["available", "uploading", "downloading"].includes(status)) {
        expect(locale[`attachment.state.hint_${status}`]).toBeTruthy();
      }
    }
  });
  it.each(fixtures.reasons)("safe failure category: $reason", ({ error, reason }) => {
    expect(attachmentFailureHint(error)).toBe(reason);
    for (const locale of locales) {
      expect(locale[`attachment.state.reason_${reason}`]).toBeTruthy();
    }
  });
});
