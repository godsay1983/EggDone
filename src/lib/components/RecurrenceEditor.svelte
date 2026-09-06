<script lang="ts">
  import { onMount } from "svelte";
  import { translator, type TranslationKey } from "$lib/i18n";
  import { recurrenceApi } from "$lib/api/recurrenceApi";
  import { saveRecurrence, stopRecurrence } from "$lib/stores/recurrenceStore";
  import { associatedRule, formSchedule, initialRuleForm, ruleEditable } from "$lib/utils/recurrenceForm";
  import { recurrenceErrorKey, recurrenceSummary } from "$lib/utils/recurrenceSummary";
  import { localDateString } from "$lib/utils/todoDates";
  import { timestampToDateTimeLocal } from "$lib/utils/reminderTimes";
  import type { Todo } from "$lib/types";
  import type { RecurrenceRule, RuleEditRequest } from "$lib/types/recurrence";

  export let todo: Todo;
  export let onClose: () => void;
  const original = { ...todo };
  const startDate = original.due_date ?? (original.due_at === null ? localDateString(0) : timestampToDateTimeLocal(original.due_at).slice(0, 10));
  let dialog: HTMLDialogElement;
  let form = initialRuleForm(startDate, null, Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC");
  let rule: RecurrenceRule | null = null;
  let device = "";
  let loaded = false;
  let busy = false;
  let error = "";
  let request: RuleEditRequest | null = null;
  let fingerprint = "";
  const days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
  $: text = (key: string) => $translator(("recurrence." + key) as TranslationKey);
  $: editable = loaded && ruleEditable(rule, original.uuid, original.repeat_series_uuid, original.repeat_rule, original.completed);
  $: first = preview(form);
  function preview(value: typeof form) {
    try { return formSchedule(value).anchor_date; } catch { return ""; }
  }
  async function load() {
    busy = true; error = "";
    try {
      const context = await recurrenceApi.context();
      rule = associatedRule(context.rules, original.uuid, original.repeat_series_uuid);
      device = context.device_id;
      form = initialRuleForm(startDate, rule, form.zone);
      loaded = true;
    } catch (e) { error = text(recurrenceErrorKey(String(e))); }
    finally { busy = false; }
  }
  onMount(() => {
    dialog.showModal();
    void load();
    return () => dialog.close();
  });
  function toggleDay(day: number) {
    form.weekdays = form.weekdays.includes(day) ? form.weekdays.filter(d => d !== day) : [...form.weekdays, day];
  }
  async function save() {
    if (busy || !editable) return;
    busy = true; error = "";
    try {
      const schedule = formSchedule(form);
      const key = JSON.stringify({ schedule, zone: form.allDay ? null : form.zone.trim() });
      if (request === null || fingerprint !== key) {
        request = { rule: { uuid: crypto.randomUUID(), first_todo_uuid: original.uuid, current_todo_uuid: original.uuid,
          current_date: schedule.anchor_date, generated_count: 1, exhausted: false, deleted_at: null,
          schedule, timezone_id: form.allDay ? null : form.zone.trim(), updated_at: Date.now(), updated_by: device },
          expected_todo_updated_at: original.updated_at, replaces: rule };
        fingerprint = key;
      }
      await saveRecurrence(request);
      onClose();
    } catch (e) { error = text(recurrenceErrorKey(String(e))); }
    finally { busy = false; }
  }
  async function stop() {
    if (busy || !rule || !editable || !window.confirm(text("stopConfirm"))) return;
    busy = true; error = "";
    try { await stopRecurrence(rule); onClose(); }
    catch (e) { error = text(recurrenceErrorKey(String(e))); }
    finally { busy = false; }
  }
</script>

<dialog bind:this={dialog} class="recurrence-editor" aria-label={text("title")} oncancel={event => { event.preventDefault(); if (!busy) onClose(); }}>
  <form onsubmit={event => { event.preventDefault(); void save(); }}>
    <header><h2>{text("title")}</h2><button type="button" class="close" aria-label={$translator("common.close")} disabled={busy} onclick={onClose}>×</button></header>
    <div class="fields">
      <strong class="task-title">{original.title}</strong>
      {#if !loaded}<p role="status">{text("loading")}</p>{/if}
      {#if rule}<p class="summary">{recurrenceSummary(rule, text)}</p>{/if}
      {#if loaded && !editable}<p>{text("blocked")}</p>{/if}
      {#if editable}
        <fieldset disabled={busy}>
          <label>{text("start")}<input type="date" bind:value={form.start} min="1900-01-01" max="9999-12-31" required /></label>
          <div class="pair">
            <label>{text("interval")}<input type="number" min="1" max="99" value={form.interval} oninput={e => form.interval = e.currentTarget.value} required /></label>
            <label>{text("frequency")}<select bind:value={form.frequency}><option value="daily">{text("daily")}</option><option value="weekly">{text("weekly")}</option><option value="monthly">{text("monthly")}</option></select></label>
          </div>
          {#if form.frequency === "weekly"}
            <div class="week" role="group" aria-label={text("weekdays")}>{#each days as day, i}<button type="button" class:active={form.weekdays.includes(i + 1)} aria-pressed={form.weekdays.includes(i + 1)} onclick={() => toggleDay(i + 1)}>{text(day)}</button>{/each}</div>
          {/if}
          {#if form.frequency === "monthly"}
            <label>{text("monthDay")}<select bind:value={form.monthDay}><option value="0">{text("lastDay")}</option>{#each Array.from({length: 31}, (_, i) => i + 1) as day}<option value={String(day)}>{day}</option>{/each}</select></label>
            <p>{text("monthHint")}</p>
          {/if}
          <label class="check"><input type="checkbox" bind:checked={form.allDay} />{text("allDay")}</label>
          {#if !form.allDay}
            <label>{text("time")}<input type="time" bind:value={form.time} required /></label>
            <label>{text("zone")}<input bind:value={form.zone} maxlength="128" required /></label>
          {/if}
          <label>{text("end")}<select bind:value={form.end}><option value="never">{text("never")}</option><option value="date">{text("date")}</option><option value="count">{text("count")}</option></select></label>
          {#if form.end === "date"}<label>{text("until")}<input type="date" bind:value={form.until} min={first || form.start} max="9999-12-31" required /></label>{/if}
          {#if form.end === "count"}<label>{text("maxCount")}<input type="number" min="1" max="100000" value={form.count} oninput={e => form.count = e.currentTarget.value} required /></label>{/if}
        </fieldset>
        <p class="summary">{text("first")}: {first || text("invalid")}</p>
        <p>{text("notice")}</p>
        {#if rule}<p>{text("replace")}</p>{/if}
      {/if}
      {#if error}<p role="alert" class="error">{error}</p>{/if}
    </div>
    <footer>
      {#if editable && rule}<button class="danger" type="button" disabled={busy} onclick={() => void stop()}>{text("stop")}</button>{/if}
      {#if !loaded && !busy}<button type="button" onclick={() => void load()}>{$translator("common.retry")}</button>{/if}
      {#if editable}<button class="active" type="submit" disabled={busy || !first}>{$translator("common.save")}</button>{/if}
    </footer>
  </form>
</dialog>

<style>
  .recurrence-editor { width: min(480px, calc(100% - 20px)); max-height: calc(100dvh - 20px); padding: 0; border: 1px solid #c7bda8; border-radius: 8px; background: #fffaf0; color: #463e31; }
  dialog::backdrop { background: #0006; }
  form { display: flex; flex-direction: column; max-height: calc(100dvh - 24px); }
  header, footer { display: flex; gap: 8px; align-items: center; justify-content: space-between; padding: 12px; flex: none; flex-wrap: wrap; }
  header { flex-wrap: nowrap; } h2 { font-size: 16px; margin: 0; } .task-title { overflow-wrap: anywhere; }
  .fields { padding: 0 14px; overflow-y: auto; min-height: 0; } fieldset { border: 0; margin: 0; padding: 0; min-width: 0; }
  label { display: grid; gap: 6px; margin-top: 12px; font-size: 13px; min-width: 0; }
  .pair { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 10px; }
  input:not([type="checkbox"]), select { width: 100%; min-width: 0; box-sizing: border-box; background: #fff; color: inherit; border: 1px solid #b3a58c; padding: 8px; border-radius: 6px; font: inherit; }
  .check { display: flex; align-items: center; } p { font-size: 12px; line-height: 1.5; overflow-wrap: anywhere; } .summary { font-weight: 600; }
  button { background: #f4ead4; color: #56442b; border: 1px solid #c7bda8; border-radius: 22px; padding: 8px 12px; font: inherit; font-size: 13px; cursor: pointer; }
  button.active { background: #f8c44e; color: #382b16; border-color: #f8c44e; } button:disabled { opacity: .5; cursor: default; }
  .close { flex: none; width: 34px; height: 34px; padding: 0; font-size: 22px; }
  .week { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 12px; } .week button { min-width: 38px; padding: 8px; }
  .danger, .error { color: #922d26; } :focus-visible { outline: 2px solid #a97100; outline-offset: 2px; }
  :global(html[data-theme="dark"]) .recurrence-editor { background: #302d27; color: #f4e7cd; border-color: #6a5842; }
  :global(html[data-theme="dark"]) .recurrence-editor button:not(.active) { background: #4a3e30; color: #f4e7cd; border-color: #6a5842; }
  :global(html[data-theme="dark"]) .recurrence-editor button.danger { background: #60342e; color: #ffe0d8; }
  :global(html[data-theme="dark"]) .recurrence-editor .error { color: #ffb7ab; }
  :global(html[data-theme="dark"]) .recurrence-editor input:not([type="checkbox"]),
  :global(html[data-theme="dark"]) .recurrence-editor select { background: #25221d; color: #f4e7cd; border-color: #8e7c60; color-scheme: dark; }
  :global(html[data-theme="dark"]) .recurrence-editor :focus-visible { outline-color: #ffd36b; }
</style>
