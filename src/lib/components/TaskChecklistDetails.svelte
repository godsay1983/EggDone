<script lang="ts">
  import { onMount } from 'svelte';
  import { translator } from '$lib/i18n';
  import { createChecklistDetailSession } from '$lib/stores/taskChecklistStore';
  import type { ChecklistItem, ChecklistPanelSnapshot } from '$lib/types/taskChecklist';
  import type { TodoGroup } from '$lib/types';
  import TaskChecklistDialog from './TaskChecklistDialog.svelte';

  export let uuid: string;
  export let groups: TodoGroup[] = [];
  export let onClose: () => void;
  export let onChanged: () => void = () => {};
  const session = createChecklistDetailSession();
  let dialog: HTMLDialogElement;
  let snapshot: ChecklistPanelSnapshot | null = null;
  let items: ChecklistItem[] = [];
  let editing = false, busy = false, loading = true, alive = false, retryable = false;
  let error: 'loadFailed' | 'toggleFailed' | 'toggleConflict' | '' = '';
  $: done = items.filter(i => i.completed).length;
  $: locked = busy || loading || !!error || !snapshot || snapshot.read_only;
  function show(node: HTMLDialogElement) { node.showModal(); return { destroy: () => node.close() }; }
  async function load(refreshParent = false) {
    loading = true; error = ''; retryable = false; snapshot = null;
    try {
      const result = await session.load(uuid);
      if (!alive) return;
      snapshot = result;
      items = result.items.items.filter(i => i.deleted_at === null).sort((a,b) => a.sort_order-b.sort_order || a.uuid.localeCompare(b.uuid));
      if (refreshParent) { try { onChanged(); } catch { /* The detail read remains valid. */ } }
    } catch { if (alive) error = 'loadFailed'; }
    finally { if (alive) loading = false; }
  }
  async function write(item: ChecklistItem | null, completed = false) {
    if (busy || loading || (item !== null && (locked || item.completed === completed))) return;
    busy = true; error = '';
    const before = items;
    if (item) items = items.map(i => i.uuid === item.uuid ? {...i, completed} : i);
    try {
      if (item) await session.setCompleted(item.uuid, completed); else await session.retry();
    } catch (e) {
      if (alive) {
        items = before; error = /STALE|READ_ONLY|MISSING/.test(String(e)) ? 'toggleConflict' : 'toggleFailed';
        retryable = error !== 'toggleConflict' && session.hasPending(); busy = false;
      }
      return;
    }
    try { onChanged(); } catch { /* A refresh callback cannot undo a committed checkbox. */ }
    if (alive) { await load(); busy = false; }
  }
  function edit() { if (!locked) editing = true; }
  function finishEdit(saved: boolean) {
    editing = false;
    if (saved) { try { onChanged(); } catch { /* Reload the detail independently. */ } }
    void load();
  }
  onMount(() => { alive = true; void load(); return () => { alive = false; }; });
</script>

{#if editing}
  <TaskChecklistDialog {uuid} {groups} onClose={() => finishEdit(false)} onSaved={() => finishEdit(true)}/>
{:else}
  <dialog bind:this={dialog} use:show aria-label={$translator('checklist.title')}
    onkeydown={e => e.stopPropagation()} oncancel={e => {e.preventDefault(); if (!busy) onClose();}}>
    <header><h2>{$translator('checklist.title')}</h2>
      <button type="button" class="edit" disabled={locked} onclick={edit}>{$translator('common.edit')}</button></header>
    <div class="body">
      {#if loading}<p role="status">{$translator('common.loading')}</p>{/if}
      {#if error}
        <p role="alert">{$translator(`checklist.${error}`)}</p>
        <div class="recovery">
          {#if retryable}<button type="button" disabled={busy} onclick={() => void write(null)}>{$translator('common.retry')}</button>{/if}
          <button type="button" disabled={busy||loading} onclick={() => void load(true)}>{$translator('checklist.reload')}</button>
        </div>
      {/if}
      {#if snapshot}
        <h3>{snapshot.title}</h3>
        {#if snapshot.note}<p class="note">{snapshot.note}</p>{/if}
        <div class="checklist-status"><span>{$translator('checklist.progress',{done,total:items.length})}</span>
          <span>{busy ? $translator('checklist.saving') : $translator('checklist.instant')}</span></div>
        <progress max={Math.max(1,items.length)} value={done} aria-label={$translator('checklist.progress',{done,total:items.length})}></progress>
        {#if snapshot.read_only}<p>{$translator('checklist.readOnly')}</p>{/if}
        {#if !items.length}<p class="empty">{$translator('checklist.empty')}</p>{/if}
        <ul>
          {#each items as item,index (item.uuid)}
            <li class:completed={item.completed}>
              <label><input type="checkbox" checked={item.completed} disabled={locked}
                aria-label={$translator('checklist.mark',{index:index+1})}
                onchange={e => void write(item,e.currentTarget.checked)}/><span>{item.content}</span></label>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
    <footer><button type="button" class="action-button done" disabled={busy} onclick={onClose}>{$translator('common.done')}</button></footer>
  </dialog>
{/if}

<style>
  dialog{--line:#e4dccb;--muted:#746853;--panel:#fffdf8;--ink:#443d31;--field:#f4f0e7;
    width:min(440px,calc(100% - 24px));max-height:calc(100% - 24px);box-sizing:border-box;padding:14px;
    border:1px solid var(--line);border-radius:8px;background:var(--panel);color:var(--ink);font-size:13px;line-height:1.45;}
  dialog[open]{display:flex;flex-direction:column;gap:12px;}dialog::backdrop{background:#0006;}
  header,footer,.checklist-status,.recovery{display:flex;align-items:center;gap:8px;flex-wrap:wrap;}
  header,.checklist-status{justify-content:space-between;}header,footer{flex-shrink:0;}footer{justify-content:flex-end;border-top:1px solid var(--line);padding-top:10px;}
  h2{font-size:15px;margin:0;font-weight:600;}h3{font-size:13px;font-weight:500;margin:0 0 6px;overflow-wrap:anywhere;}
  .body{overflow:auto;min-height:0;scrollbar-width:thin;}.note{white-space:pre-wrap;overflow-wrap:anywhere;margin:0 0 12px;}
  .checklist-status{color:var(--muted);font-size:12px;}.checklist-status span:last-child{font-size:11px;}
  progress{width:100%;height:3px;display:block;margin:8px 0;accent-color:#f4c356;}
  ul{list-style:none;padding:0;margin:0;}li{border-bottom:1px solid var(--line);}
  label{display:flex;align-items:flex-start;gap:8px;min-height:38px;padding:8px 2px;box-sizing:border-box;cursor:pointer;}
  input{flex-shrink:0;width:16px;height:16px;margin:1px 0;accent-color:#e8b63f;}label span{min-width:0;overflow-wrap:anywhere;}
  .completed span{text-decoration:line-through;color:var(--muted);}
  button{font:inherit;font-size:12px;min-height:32px;padding:5px 12px;border:1px solid var(--line);border-radius:6px;background:var(--field);color:var(--ink);cursor:pointer;}
  button.edit{border-color:transparent;background:transparent;}button.done{background:#f4c356;border-color:#f4c356;color:#382b15;}
  button:disabled,input:disabled{opacity:.5;cursor:default;}button:focus-visible,input:focus-visible{outline:2px solid #bc8b1a;outline-offset:2px;}
  .empty{color:var(--muted);padding:12px 0;}[role=alert]{color:var(--ink);margin:0 0 8px;}
  @media (pointer:coarse){button,label{min-height:44px;}}
  :global(:root[data-theme='dark']) dialog{--line:#514b3d;--muted:#c1b7a0;--panel:#28251e;--ink:#f3e8d0;--field:#393328;}
</style>
