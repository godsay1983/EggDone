<script lang="ts">
  import { onMount } from 'svelte';
  import { translator, type TranslationKey } from '$lib/i18n';
  import { createTemplateLibrarySession } from '$lib/stores/taskTemplateStore';
  import { taskChecklistEditorApi } from '$lib/api/taskChecklistEditorApi';
  import { templateFromTask, templateCreation, templateError } from '$lib/utils/templateLibrarySession';
  import type { TaskTemplate, TemplateContent } from '$lib/types/taskTemplate';
  import type { TaskCopyDraft } from '$lib/utils/taskCopyDraft';
  import type { TodoGroup } from '$lib/types';
  export let sourceUuid = '';
  export let groups: TodoGroup[] = [];
  export let onClose: () => void;
  export let onUse: (draft: TaskCopyDraft) => void;
  const session = createTemplateLibrarySession();
  let dialog: HTMLDialogElement, alive = true;
  let rows: TaskTemplate[] = [], selected: TaskTemplate | null = null;
  let search = '', name = '', title = '', note = '', group = '', lines = '';
  let detail = false, editing = false, busy = false, error = '', loading = true, confirmDelete = false, saved = false;
  $: matches = rows.filter(row => [row.content.name,row.content.title,row.content.note,...row.content.checklist].join(' ').toLowerCase().includes(search.trim().toLowerCase()));
  $: missing = group !== '' && !groups.some(g => g.uuid === group);
  $: text = (key: string) => $translator(('templates.' + key) as TranslationKey);
  function fill(c: TemplateContent) { name=c.name;title=c.title;note=c.note;group=c.group_uuid??'';lines=c.checklist.join('\n'); }
  function content(): TemplateContent { return {name,title,note,group_uuid:group||null,checklist:lines.trim()?lines.split(/\r?\n/):[]}; }
  function select(row: TaskTemplate) { session.select(row);selected=row;fill(row.content);detail=true;editing=false;error='';saved=false;confirmDelete=false; }
  async function load(initial=false) {
    loading=true;error='';saved=false;confirmDelete=false;detail=false;editing=false;
    try {
      const next=await session.list();if(!alive)return;rows=next;
      if(initial&&sourceUuid){
        const source=await taskChecklistEditorApi.read(sourceUuid);if(!alive)return;
        if(source.task.todo_uuid!==sourceUuid)throw Error('INVALID_SOURCE');
        session.select(null);selected=null;fill(templateFromTask(source));detail=true;editing=true;
      }
    }catch(e){if(alive)error=templateError(String(e));}finally{if(alive)loading=false;}
  }
  onMount(()=>{dialog.showModal();void load(true);return()=>{alive=false;dialog.close();};});
  async function save(deleted=false) {
    if(busy||loading)return;
    busy=true;error='';saved=false;
    try{
      const row=await session.save(deleted&&selected?selected.content:content(),deleted);if(!alive)return;
      rows=rows.filter(r=>r.uuid!==row.uuid);
      if(!deleted){rows=[row,...rows];selected=row;fill(row.content);editing=false;saved=true;}
      else{detail=false;selected=null;}
      confirmDelete=false;
    }catch(e){if(alive)error=templateError(String(e));}finally{if(alive)busy=false;}
  }
  function use() {
    if(busy||loading||editing)return;
    try{const draft=templateCreation(content(),groups.map(g=>g.uuid),()=>crypto.randomUUID());onUse(draft);}
    catch(e){error=templateError(String(e));}
  }
</script>

<dialog bind:this={dialog} aria-label={text('title')} onkeydown={e=>e.stopPropagation()} oncancel={e=>{e.preventDefault();if(!busy)onClose();}}>
  <header><h2>{text(detail?(editing?'saveAs':'preview'):'title')}</h2>
    <button type="button" class="action-button" disabled={busy} onclick={onClose}>{text('close')}</button></header>
  <section>
    {#if loading}<p role="status">{text('loading')}</p>{/if}
    {#if error}<p role="alert">{text(error)}</p><button class="action-button" type="button" disabled={busy} onclick={()=>void load(!detail&&!!sourceUuid)}>{text('reload')}</button>{/if}
    {#if saved}<p role="status">{text('saved')}</p>{/if}
    {#if !loading && !detail}
      <input type="search" bind:value={search} placeholder={text('search')} aria-label={text('search')}/>
      {#if !matches.length}<p class="empty">{text(rows.length?'noMatch':'empty')}</p>{/if}
      <ul>{#each matches as row (row.uuid)}<li><button class="template-row" type="button" onclick={()=>select(row)}>
        <strong>{row.content.name}</strong><span>{row.content.title}</span><small>{row.content.checklist.length} {$translator('checklist.title')}</small>
      </button></li>{/each}</ul>
    {:else if !loading}
      {#if editing}
        <label>{text('name')}<input bind:value={name} maxlength="60" disabled={busy}/></label>
        <label>{text('taskTitle')}<input bind:value={title} maxlength="100" disabled={busy}/></label>
        <label>{text('note')}<textarea rows="2" bind:value={note} maxlength="1000" disabled={busy}></textarea></label>
      {:else}
        <h3>{name}</h3><p>{title}</p>{#if note}<p class="note">{note}</p>{/if}
      {/if}
      {#if missing}<p role="alert">{text('missingGroup')}</p>{/if}
      <label>{text('group')}<select bind:value={group} disabled={busy} aria-label={text('group')}>
        {#if missing}<option value={group}>{text('missingGroup')}</option>{/if}
        <option value="">{text('ungrouped')}</option>{#each groups as g}<option value={g.uuid}>{g.name}</option>{/each}
      </select></label>
      {#if editing}<label>{text('checklist')}<textarea rows="5" bind:value={lines} disabled={busy}></textarea></label>
      {:else}<ol>{#each content().checklist as item}<li>{item}</li>{/each}</ol>{/if}
      {#if confirmDelete}<p role="alert">{text('confirmDelete')}</p>{/if}
    {/if}
  </section>
  <footer>
    {#if detail}
      <button class="action-button" type="button" disabled={busy} onclick={()=>{detail=false;editing=false;error='';saved=false;confirmDelete=false;}}>{text('back')}</button>
      {#if editing}
        <button class="action-button" data-tone="primary" type="button" disabled={busy||missing} onclick={()=>void save()}>{text('save')}</button>
      {:else if confirmDelete}
        <button class="action-button" type="button" disabled={busy} onclick={()=>confirmDelete=false}>{text('cancel')}</button>
        <button class="action-button" data-tone="danger" type="button" disabled={busy} onclick={()=>void save(true)}>{text('confirm')}</button>
      {:else}
        <button class="action-button" type="button" disabled={busy} onclick={()=>{editing=true;saved=false;}}>{text('edit')}</button>
        <button class="action-button" data-tone="danger" type="button" disabled={busy} onclick={()=>confirmDelete=true}>{text('delete')}</button>
        <button class="action-button" data-tone="primary" type="button" disabled={busy||missing} onclick={use}>{text('use')}</button>
      {/if}
    {/if}
  </footer>
</dialog>

<style>
  dialog{--panel:#fffdf7;--field:#f5f2e9;--line:#d6d0bd;--muted:#6c665b;--ink:#302c23;background:var(--panel);color:var(--ink);color-scheme:light;
    width:min(480px,calc(100% - 24px));max-height:calc(100% - 24px);padding:16px;border:1px solid var(--line);border-radius:8px;box-sizing:border-box;font-size:13px;}
  dialog[open]{display:flex;flex-direction:column;gap:12px;}dialog::backdrop{background:#0006;}
  header,footer{display:flex;align-items:center;gap:8px;flex-wrap:wrap;flex-shrink:0;}header h2{flex:1;}
  h2{font-size:15px;margin:0;}h3{font-size:14px;margin:0;}section{min-height:0;overflow:auto;display:flex;flex-direction:column;gap:10px;}
  footer{justify-content:flex-end;}button{font:inherit;}header button,footer button{min-height:32px;}
  label{display:grid;gap:4px;}input,textarea,select{min-width:0;width:100%;box-sizing:border-box;font:inherit;color:var(--ink);background:var(--field);border:1px solid var(--line);border-radius:6px;padding:8px;}
  option{color:var(--ink);background:var(--panel);}textarea{resize:vertical;}ul{list-style:none;padding:0;margin:0;}
  .template-row{display:flex;flex-direction:column;gap:4px;text-align:left;width:100%;padding:10px 4px;background:transparent;color:inherit;border:0;border-bottom:1px solid var(--line);cursor:pointer;}
  .template-row:hover{background:var(--field);}strong,span,small,p,li{overflow-wrap:anywhere;}small,.empty{color:var(--muted);}
  p{margin:0;line-height:1.5;}.note{white-space:pre-wrap;}p[role=alert]{border-left:3px solid #bc713e;padding-left:8px;}
  ol{margin:0;padding-left:24px;}ol li{padding:6px 0;}button:focus-visible{outline:2px solid #b28a19;outline-offset:-2px;}
  @media(pointer:coarse){header button,footer button{min-height:44px;}}
  :global(html[data-theme=dark]) dialog{--panel:#29261f;--field:#363128;--line:#454034;--muted:#c2b59c;--ink:#f1e7d3;color-scheme:dark;}
</style>
