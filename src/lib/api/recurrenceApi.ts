import { invoke } from "@tauri-apps/api/core";
import type { RecurrenceRule, RuleEditRequest } from "$lib/types/recurrence";
export interface RecurrenceContext { rules: RecurrenceRule[]; device_id: string }
export const recurrenceApi = {
  context: () => invoke<RecurrenceContext>("recurrence_editor_context"),
  save: (request: RuleEditRequest) => invoke<RecurrenceRule>("save_recurrence_rule", { request }),
  stop: (expected: RecurrenceRule) => invoke<RecurrenceRule>("stop_recurrence_rule", { expected }),
};
