<script lang="ts">
  import { onMount } from "svelte";
  import { translator } from "$lib/i18n";
  import { captureContent, captureTitle, normalizeCapture, type CaptureDraft } from "$lib/utils/capture";
  import { parseQuickAdd } from "$lib/utils/quickAdd";

  export let draft: CaptureDraft;
  export let groups: string[] = [];
  export let onSave: (draft: CaptureDraft, parse: boolean) => Promise<void>;
  export let onCancel: () => void;
  let title = draft.title;
  let body = draft.body;
  let target = draft.target;
  let recognize = false;
  let busy = false;
  let error = "";
  let dialog: HTMLDialogElement;
  $: current = normalizeCapture({ ...draft, title, body, target });
  $: tooLong = captureContent(current).length > (target === "todo" ? 1000 : 20000);
  $: parsed = parseQuickAdd(captureTitle(current), new Date(), groups);
  onMount(() => {
    dialog.showModal();
    return () => dialog.close();
  });
  async function save() {
    if (busy || tooLong || !captureTitle(current)) return;
    busy = true;
    error = "";
    try { await onSave(current, recognize); }
    catch { error = $translator("capture.saveFailed"); }
    finally { busy = false; }
  }
</script>

<dialog bind:this={dialog} class="capture-dialog" oncancel={(event) => {
  event.preventDefault();
  if (!busy) onCancel();
}}>
  <form onsubmit={(event) => { event.preventDefault(); void save(); }}>
    <header>
      <h2>{$translator("capture.title")}</h2>
      <button class="capture-close" type="button" disabled={busy} aria-label={$translator("common.close")} onclick={onCancel}>×</button>
    </header>
    <div class="capture-fields">
      <p>{$translator("capture.pending")}</p>
      <div class="capture-targets" role="group" aria-label={$translator("capture.target")}>
        <button type="button" class:active={target === "todo"} aria-pressed={target === "todo"} disabled={busy} onclick={() => target = "todo"}>{$translator("capture.todo")}</button>
        <button type="button" class:active={target === "note"} aria-pressed={target === "note"} disabled={busy} onclick={() => target = "note"}>{$translator("capture.note")}</button>
      </div>
      <label>{$translator("capture.heading")}<input bind:value={title} maxlength="100" disabled={busy} /></label>
      <label>{$translator("capture.body")}<textarea bind:value={body} maxlength="20000" disabled={busy} rows="6"></textarea></label>
      {#if draft.source_url}<p class="capture-url">{draft.source_url}</p>{/if}
      {#if target === "todo"}
        <label class="capture-recognize"><input type="checkbox" bind:checked={recognize} disabled={busy} />{$translator("capture.recognize")}</label>
        {#if recognize}<p>{parsed.title}{parsed.label ? " · " + parsed.label : ""}{parsed.groupName ? " · " + parsed.groupName : ""}</p>{/if}
      {/if}
      {#if draft.truncated}<p role="status">{$translator("capture.truncated")}</p>{/if}
      {#if tooLong}<p role="alert">{$translator("capture.tooLong")}</p>{/if}
      {#if error}<p role="alert">{error}</p>{/if}
    </div>
    <footer>
      <button type="button" disabled={busy} onclick={onCancel}>{$translator("common.cancel")}</button>
      <button class="active" type="submit" disabled={busy || tooLong || !captureTitle(current)}>{$translator(target === "todo" ? "capture.saveTodo" : "capture.saveNote")}</button>
    </footer>
  </form>
</dialog>

<style>
  .capture-dialog { width: min(540px, calc(100% - 24px)); max-height: calc(100dvh - 24px); padding: 0; border: 1px solid #d5c8ac; border-radius: 8px; background: #fffaf0; color: #463e31; }
  .capture-dialog::backdrop { background: #0006; }
  form { display: flex; flex-direction: column; max-height: calc(100dvh - 28px); }
  header, footer { display: flex; gap: 12px; align-items: center; justify-content: space-between; padding: 14px 16px; flex: none; }
  h2 { margin: 0; font-size: 18px; }
  .capture-fields { overflow-y: auto; min-height: 0; padding: 0 16px 12px; }
  p { font-size: 13px; line-height: 1.5; overflow-wrap: anywhere; }
  label { display: grid; gap: 6px; margin-top: 14px; font-size: 13px; }
  input:not([type="checkbox"]), textarea { width: 100%; min-width: 0; border: 1px solid #c7bda8; border-radius: 6px; padding: 10px; background: #fff; color: inherit; font: inherit; }
  textarea { resize: vertical; min-height: 100px; }
  button { border: 1px solid #d5c8ac; background: #f4ead4; color: #56442b; border-radius: 22px; padding: 9px 14px; font: inherit; font-size: 13px; cursor: pointer; }
  button.active { background: #f8c44e; color: #382b16; border-color: #f8c44e; }
  button:disabled { opacity: .5; cursor: default; }
  .capture-close { width: 36px; height: 36px; padding: 0; font-size: 23px; flex: none; }
  .capture-targets { display: flex; gap: 8px; }
  .capture-targets button { flex: 1; }
  .capture-recognize { display: flex; align-items: center; }
  input:focus-visible, textarea:focus-visible, button:focus-visible { outline: 2px solid #a97100; outline-offset: 2px; }
  :global(html[data-theme="dark"]) .capture-dialog { background: #302d27; color: #f4e7cd; border-color: #5c5140; }
  :global(html[data-theme="dark"]) .capture-dialog button:not(.active) { background: #4a3e30; color: #f4e7cd; border-color: #6a5842; }
  :global(html[data-theme="dark"]) .capture-dialog input:not([type="checkbox"]),
  :global(html[data-theme="dark"]) .capture-dialog textarea { background: #25221d; border-color: #6a5842; }
  :global(html[data-theme="dark"]) .capture-dialog :focus-visible { outline-color: #ffd36b; }
</style>
