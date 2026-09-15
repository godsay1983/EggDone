<script lang="ts">
  import { onMount } from 'svelte';
  import { translator, type TranslationKey } from '$lib/i18n';
  import { BatchCreationSession, batchError } from '$lib/utils/batchCreationSession';
  import { refreshAfterBatch } from '$lib/stores/taskBatchStore';
  import type { TodoGroup } from '$lib/types';
  export let session: BatchCreationSession;
  export let groups: TodoGroup[] = [];
  export let onClose: () => void;
  let dialog: HTMLDialogElement;
  let error = '', refreshing = false, refreshFailed = false, pasting = false, alive = true;
  $: text = (key: string) => $translator(('batch.' + key) as TranslationKey);
  $: locked = session.locked();
  $: missing = session.group !== '' && !groups.some(g => g.uuid === session.group);
  $: count = session.count();
  function update(action: () => void) {
    error = '';
    try { action(); } catch (e) { error = batchError(String(e)); }
    session = session;
  }
  function close() { if (!session.busy && !refreshing && !pasting) onClose(); }
  onMount(() => { if (session.locked() && !session.done) error = 'failed'; dialog.showModal(); return () => { alive = false; dialog.close(); }; });
  async function paste() {
    if (locked || pasting) return;
    pasting = true;
    const before = session.text;
    try {
      const value = await navigator.clipboard.readText();
      if (alive && !session.locked() && session.text === before) {
        if (!value.trim()) error = 'pasteFailed';
        else update(() => session.input(before + (before ? '\n' : '') + value));
      }
    } catch { if (alive) error = 'pasteFailed'; }
    finally { if (alive) pasting = false; }
  }
  async function refresh() {
    refreshing = true; refreshFailed = false;
    try { await refreshAfterBatch(); } catch { if (alive) refreshFailed = true; }
    finally { if (alive) refreshing = false; }
  }
  async function submit() {
    if (session.busy || session.done) return;
    error = '';
    const pending = session.submit(groups.map(g => g.uuid)); session = session;
    try { await pending; session = session; if (alive) await refresh(); }
    catch (e) { if (alive) error = batchError(String(e)); }
    finally { if (alive) session = session; }
  }
</script>

<dialog bind:this={dialog} aria-label={text('title')} onkeydown={e => e.stopPropagation()} oncancel={e => { e.preventDefault(); close(); }}>
  <header><h2>{text('title')}</h2><button class="action-button" disabled={session.busy || refreshing || pasting} onclick={close}>{text(session.done ? 'close' : 'cancel')}</button></header>
  <section>
    {#if session.done}
      <p role="status">{text('saved').replace('{count}', String(count))}</p>
      {#if refreshFailed}<p role="alert">{text('refreshFailed')}</p><button class="action-button" disabled={refreshing} onclick={() => void refresh()}>{text('refresh')}</button>{/if}
    {:else}
      {#if error}<p role="alert">{text(error)}</p>{/if}
      {#if !session.reviewing}
        <label for="batch-text">{text('input')}</label>
        <textarea id="batch-text" rows="8" value={session.text} placeholder={text('placeholder')} disabled={pasting}
          oninput={e => update(() => session.input(e.currentTarget.value))}></textarea>
        <div class="tools"><span>{session.text.length} / 20000</span><button class="action-button" disabled={pasting} onclick={() => void paste()}>{text('paste')}</button></div>
      {:else}
        <label for="batch-group">{text('group')}</label>
        <select id="batch-group" value={session.group} disabled={locked} onchange={e => update(() => session.selectGroup(e.currentTarget.value))}>
          {#if missing}<option value={session.group}>{text('missingGroup')}</option>{/if}
          <option value="">{text('ungrouped')}</option>{#each groups as g}<option value={g.uuid}>{g.name}</option>{/each}
        </select>
        <p class="muted">{text('note')}</p>
        <div class="tools"><span>{text('selected').replace('{count}', String(count)).replace('{total}', String(session.preview.rows.length))}</span>
          <button class="text-button" disabled={locked} onclick={() => update(() => session.selectAll(true))}>{text('all')}</button>
          <button class="text-button" disabled={locked} onclick={() => update(() => session.selectAll(false))}>{text('none')}</button></div>
        <ol class="rows">{#each session.preview.rows as row (row.lineNumber)}
          <li class:unselected={!row.selected}>
            <label class="check"><input type="checkbox" checked={row.selected} disabled={locked}
              aria-label={text('line').replace('{line}', String(row.lineNumber))}
              onchange={e => update(() => session.change(row.lineNumber, row.title, e.currentTarget.checked))}/><span>{row.lineNumber}</span></label>
            <div class="row-content"><textarea rows="1" value={row.title} disabled={locked} aria-invalid={row.selected && !!row.issue}
              aria-label={text('line').replace('{line}', String(row.lineNumber))}
              oninput={e => update(() => session.change(row.lineNumber, e.currentTarget.value, row.selected))}></textarea>
              {#if row.issue}<small class:invalid={row.selected}>{text(row.issue)}</small>{:else if row.duplicate}<small>{text('duplicate')}</small>{/if}
            </div>
          </li>
        {/each}</ol>
      {/if}
    {/if}
  </section>
  {#if !session.done}<footer>
    {#if session.reviewing}
      <button class="action-button" disabled={locked} onclick={() => update(() => session.back())}>{text('back')}</button>
      <button class="action-button" data-tone="primary" disabled={session.busy || (!locked && (missing || !!session.issue()))} onclick={() => void submit()}>
        {text(locked ? 'retry' : 'create').replace('{count}', String(count))}</button>
    {:else}<button class="action-button" data-tone="primary" disabled={pasting || !session.text.trim()} onclick={() => update(() => session.review())}>{text('preview')}</button>{/if}
  </footer>{/if}
</dialog>

<style>
  dialog{--panel:#fffdf7;--field:#f5f2e9;--line:#d6d0bd;--muted:#6c665b;--ink:#302c23;--error:#a3372b;color-scheme:light;
    background:var(--panel);color:var(--ink);width:min(520px,calc(100% - 24px));max-height:calc(100% - 24px);padding:16px;box-sizing:border-box;border:1px solid var(--line);border-radius:8px;font-size:13px;}
  dialog[open]{display:flex;flex-direction:column;gap:12px;}dialog::backdrop{background:#0006;}
  header,footer,.tools{display:flex;flex-wrap:wrap;align-items:center;gap:8px;flex-shrink:0;}header h2,.tools>span{flex:1;min-width:0;}h2{font-size:15px;margin:0;overflow-wrap:anywhere;}
  section{display:flex;flex-direction:column;gap:8px;min-height:0;overflow:auto;}footer{justify-content:flex-end;}button{font:inherit;min-height:32px;white-space:normal;}
  textarea,select{width:100%;box-sizing:border-box;min-width:0;background:var(--field);color:var(--ink);border:1px solid var(--line);border-radius:6px;font:inherit;padding:8px;}
  option{background:var(--panel);color:var(--ink);}textarea{resize:vertical;line-height:1.5;}p{margin:0;line-height:1.5;overflow-wrap:anywhere;}.muted,small,.tools>span{color:var(--muted);}
  .rows{list-style:none;margin:0;padding:0;}.rows li{display:flex;align-items:flex-start;gap:8px;padding:8px 0;border-bottom:1px solid var(--line);}
  .check{display:flex;align-items:center;gap:4px;min-height:36px;flex-shrink:0;}.check input{width:16px;height:16px;accent-color:#96721b;}.check span{min-width:2ch;color:var(--muted);font-variant-numeric:tabular-nums;}
  .row-content{min-width:0;flex:1;}small{display:block;overflow-wrap:anywhere;}.invalid,p[role=alert]{color:var(--error);}.unselected textarea{color:var(--muted);}
  .text-button{background:transparent;color:var(--ink);border:0;padding:4px;cursor:pointer;}button:disabled{opacity:.5;cursor:default;}
  :focus-visible{outline:2px solid #a48121;outline-offset:1px;}
  textarea:focus-visible,select:focus-visible{outline:1px solid #a48121;outline-offset:-1px;border-color:#a48121;}
  @media(pointer:coarse){button,.check{min-height:44px;}}
  :global(html[data-theme=dark]) dialog{--panel:#29261f;--field:#363128;--line:#454034;--muted:#c2b59c;--ink:#f1e7d3;--error:#ffb4a6;color-scheme:dark;}
</style>
