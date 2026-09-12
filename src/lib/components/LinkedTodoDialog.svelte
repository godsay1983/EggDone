<script lang="ts">
  import { onMount } from "svelte";
  import { translator } from "$lib/i18n";
  import { dateTimeLocalToTimestamp, timestampToDateTimeLocal } from "$lib/utils/reminderTimes";
  import type { LinkedTodoDraft } from "$lib/types/taskNoteLink";
  export let noteUuid: string;
  export let initialTitle: string;
  export let onSave: (draft: LinkedTodoDraft) => Promise<void>;
  export let onCancel: () => void;
  const uuid = crypto.randomUUID();
  let title = initialTitle;
  let body = "";
  let dueDate = "";
  let reminder = "";
  let busy = false;
  let failed = false;
  let dialog: HTMLDialogElement;
  onMount(() => { dialog.showModal(); return () => dialog.close(); });
  async function save() {
    if (busy || !title.trim()) return;
    busy = true; failed = false;
    try {
      const reminderAt = reminder ? dateTimeLocalToTimestamp(reminder) : null;
      if (reminder && (reminderAt === null || timestampToDateTimeLocal(reminderAt) !== reminder || reminderAt <= Date.now())) {
        throw Error("invalid reminder");
      }
      await onSave({ todo_uuid: uuid, note_uuid: noteUuid, title: title.trim(), note: body,
        due_date: dueDate || null, due_at: null, reminder_at: reminderAt, group_uuid: null, priority: 0 });
    } catch { failed = true; } finally { busy = false; }
  }
</script>

<dialog bind:this={dialog} onkeydown={event => event.stopPropagation()} oncancel={event => {
  event.preventDefault(); if (!busy) onCancel();
}}>
  <form onsubmit={event => { event.preventDefault(); void save(); }}>
    <h2>{$translator("links.create")}</h2>
    <div class="fields">
      <label>{$translator("links.title")}<input bind:value={title} maxlength="100" required disabled={busy} /></label>
      <label>{$translator("capture.body")}<textarea bind:value={body} maxlength="1000" rows="3" disabled={busy}></textarea></label>
      <label>{$translator("links.date")}<input type="date" bind:value={dueDate} disabled={busy} /></label>
      <label>{$translator("links.reminder")}<input type="datetime-local" bind:value={reminder} disabled={busy} /></label>
      {#if failed}<p role="alert">{$translator("links.saveFailed")}</p>{/if}
    </div>
    <footer>
      <button type="button" class="action-button" disabled={busy} onclick={onCancel}>{$translator("common.cancel")}</button>
      <button type="submit" class="action-button" data-tone="primary" disabled={busy || !title.trim()}>{$translator("capture.saveTodo")}</button>
    </footer>
  </form>
</dialog>

<style>
  dialog { --link-border: #d5c8ac; width: min(480px, calc(100% - 24px)); padding: 16px; max-height: calc(100% - 24px); box-sizing: border-box; border: 1px solid var(--link-border); border-radius: 8px; background: #fffaf0; color: #463e31; }
  dialog[open] { display: flex; flex-direction: column; }
  dialog::backdrop { background: #0006; }
  form { display: flex; flex-direction: column; gap: 12px; min-height: 0; }
  h2 { font-size: 18px; margin: 0; }
  .fields { min-height: 0; overflow: auto; display: grid; gap: 12px; }
  label { display: grid; gap: 6px; font-size: 14px; }
  input, textarea { width: 100%; min-width: 0; box-sizing: border-box; padding: 8px; border: 1px solid var(--link-border); border-radius: 6px; font: inherit; color: inherit; background: transparent; }
  footer { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: 8px; }
  p { font-size: 13px; margin: 0; }
  :global(html[data-theme="dark"]) dialog { --link-border: #6a5842; background: #302d27; color: #f4e7cd; color-scheme: dark; }
</style>
