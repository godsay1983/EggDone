import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { formSchedule, initialRuleForm, ruleEditable, associatedRule, visibleRecurrenceRule, type RuleForm } from "./recurrenceForm";
import { recurrenceSummary, recurrenceErrorKey } from "./recurrenceSummary";
import { translate, type TranslationKey } from "$lib/i18n";
import type { RecurrenceRule } from "$lib/types/recurrence";
const fixtures = JSON.parse(readFileSync("docs/fixtures/recurrence-editor-form.json", "utf8")) as {
  base: RuleForm; cases: { id: string; change: Partial<RuleForm>; anchor?: string; error?: boolean }[];
};
describe("shared recurrence editor form", () => {
  for (const c of fixtures.cases) it(c.id, () => {
    const form = { ...fixtures.base, ...c.change };
    if (c.error) expect(() => formSchedule(form)).toThrow(/INVALID_RECURRENCE/);
    else {
      const result = formSchedule(form);
      expect(result.anchor_date).toBe(c.anchor);
      expect(result.weekdays).toEqual([...result.weekdays].sort((a, b) => a - b));
      if (form.allDay) expect(result.local_time_minutes).toBeNull();
    }
  });
  const rule: RecurrenceRule = { uuid: "rule", first_todo_uuid: "first", current_todo_uuid: "current", current_date: "2026-09-06",
    schedule: formSchedule(fixtures.base), timezone_id: null, generated_count: 2, exhausted: false, updated_at: 2, updated_by: "device", deleted_at: null };
  it("allows ordinary and active current tasks only", () => {
    expect(ruleEditable(null, "ordinary", null, null, false)).toBe(true);
    expect(ruleEditable(rule, "current", "first", null, false)).toBe(true);
    expect(ruleEditable(rule, "first", "first", null, false)).toBe(false);
    expect(ruleEditable(null, "ordinary", null, "daily", false)).toBe(false);
    expect(ruleEditable(null, "ordinary", "unknown", null, false)).toBe(false);
    expect(ruleEditable(rule, "current", "first", null, true)).toBe(false);
    expect(ruleEditable({ ...rule, deleted_at: 3 }, "current", "first", null, false)).toBe(false);
    expect(ruleEditable({ ...rule, exhausted: true }, "current", "first", null, false)).toBe(false);
  });
  it("rebase from current date and prefer active replacement over tombstone", () => {
    expect(initialRuleForm("2026-01-01", rule, "UTC").start).toBe(rule.current_date);
    const stopped = { ...rule, uuid: "old", updated_at: 99, deleted_at: 99 };
    expect(associatedRule([stopped, rule], "current", "first")?.uuid).toBe("rule");
    expect(associatedRule([stopped], "current", "first")?.deleted_at).toBe(99);
  });
  it("summaries and actionable errors are bilingual", () => {
    for (const locale of ["en-US", "zh-CN"] as const) {
      const text = (key: string) => translate(locale, ("recurrence." + key) as TranslationKey);
      expect(recurrenceSummary({ ...rule, deleted_at: 3 }, text)).toContain(text("stopped"));
      expect(recurrenceSummary({ ...rule, exhausted: true }, text)).toContain(text("exhausted"));
      expect(recurrenceSummary(rule, text)).not.toMatch(/undefined|\{n\}/);
    }
    expect(recurrenceErrorKey("RECURRENCE_EDIT_CONFLICT")).toBe("conflict");
    expect(recurrenceErrorKey("RECURRENCE_TIMEZONE_UNSUPPORTED")).toBe("timezoneError");
  });
  it.each([
    { deleted_at: 99, exhausted: false },
    { deleted_at: null, exhausted: true },
  ])("hides detached historical rule without deleting it: %j", (state) => {
    const stopped = { ...rule, ...state };
    const before = JSON.stringify(stopped);
    expect(visibleRecurrenceRule([stopped], "current", null, null)).toBeNull();
    expect(visibleRecurrenceRule([stopped], "first", null, null)).toBeNull();
    expect(associatedRule([stopped], "current", null)).toBe(stopped);
    expect(JSON.stringify(stopped)).toBe(before);
  });
  it.each([
    { deleted_at: 99, exhausted: false },
    { deleted_at: null, exhausted: true },
  ])("retains historical badge while series is still linked: %j", (state) => {
    const stopped = { ...rule, ...state };
    expect(visibleRecurrenceRule([stopped], "current", "first", null)).toBe(stopped);
  });
  it("keeps active replacement visible and ignores history for standard repeats", () => {
    const stopped = { ...rule, uuid: "old", updated_at: 99, deleted_at: 99 };
    expect(visibleRecurrenceRule([stopped, rule], "current", null, null)).toBe(rule);
    expect(visibleRecurrenceRule([stopped, rule], "current", "first", "daily")).toBeNull();
    expect(visibleRecurrenceRule([], "current", "unknown", null)).toBeNull();
    expect(visibleRecurrenceRule([], "ordinary", null, null)).toBeNull();
  });
  it("task badges and schedule controls use the visible summary, not historical lookup", () => {
    const source = readFileSync("src/lib/components/TodoItem.svelte", "utf8");
    expect(source).toContain("visibleRecurrenceRule($recurrenceRules, todo.uuid, todo.repeat_series_uuid, todo.repeat_rule)");
    expect(source).toContain('disabled={scheduleSaving || Boolean(customSummary)}');
    expect(source).toContain('todo.repeat_rule === null && todo.repeat_series_uuid !== null ? $translator("recurrence.unknown") : ""');
  });
});
