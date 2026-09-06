import { writable } from "svelte/store";
import { isTauri } from "@tauri-apps/api/core";
import { recurrenceApi } from "$lib/api/recurrenceApi";
import type { RecurrenceRule, RuleEditRequest } from "$lib/types/recurrence";
import { todos } from "./todoStore";
import { scheduleAutoSync } from "$lib/sync/autoSync";

export const recurrenceRules = writable<RecurrenceRule[]>([]);
let generation = 0;
export async function refreshRecurrenceRules() {
  if (!isTauri()) return;
  const current = ++generation;
  try {
    const context = await recurrenceApi.context();
    if (current === generation) recurrenceRules.set(context.rules);
  } catch {
    if (current === generation) recurrenceRules.set([]);
  }
}
export async function saveRecurrence(request: RuleEditRequest) {
  await recurrenceApi.save(request);
  await todos.refresh();
  await refreshRecurrenceRules();
  scheduleAutoSync();
}
export async function stopRecurrence(rule: RecurrenceRule) {
  await recurrenceApi.stop(rule);
  await todos.refresh();
  await refreshRecurrenceRules();
  scheduleAutoSync();
}
