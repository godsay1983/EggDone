<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { translator } from '$lib/i18n';
  import { createChecklistSession } from '$lib/stores/taskChecklistStore';
  import type { ChecklistEdit } from '$lib/types/taskChecklist';
  export let uuid: string;
  export let onClose: () => void;
  export let onSaved: () => void;
  const session = createChecklistSession();
  let dialog: HTMLDialogElement;
  let title='', note='', items: ChecklistEdit[]=[], busy=false, loading=true, loaded=false, readOnly=false;
  let expandedItem = '';
  let detailsOpen = false;
  function fitText(node: HTMLTextAreaElement, _content: string) {
    const resize = () => { node.style.height = 'auto'; node.style.height = (node.scrollHeight + 2) + 'px'; };
    let width = 0;
    const observer = new ResizeObserver(() => {
      if (node.clientWidth !== width) { width = node.clientWidth; resize(); }
    });
    observer.observe(node);
    const frame = requestAnimationFrame(resize);
    return { update: resize, destroy: () => { observer.disconnect(); cancelAnimationFrame(frame); } };
  }
  let error: 'loadFailed' | 'saveFailed' | 'conflict' | 'invalid' | ''='';
  $: done=items.filter(i=>i.completed).length;
  async function load() {
    loading=true;error='';
    try {
      const s=await session.load(uuid);title=s.title;note=s.note;readOnly=s.read_only;
      items=s.items.items.filter(i=>i.deleted_at===null).sort((a,b)=>a.sort_order-b.sort_order||a.uuid.localeCompare(b.uuid))
        .map(i=>({uuid:i.uuid,content:i.content,completed:i.completed,sort_order:i.sort_order}));
      loaded=true;
    } catch {error='loadFailed';} finally {loading=false;}
  }
  onMount(()=>{dialog.showModal();void load();return ()=>dialog.close();});
  async function add() {
    if(items.length>=20)return;
    items=[...items,{uuid:crypto.randomUUID(),content:'',completed:false,sort_order:(items.length+1)*1000}];
    await tick(); dialog.querySelector<HTMLTextAreaElement>('li:last-child textarea')?.focus();
  }
  function move(index:number,offset:number) {
    const next=[...items];[next[index],next[index+offset]]=[next[index+offset],next[index]];items=next;
  }
  async function save() {
    if(busy||loading||!loaded||readOnly)return;
    if(!title.trim()||items.some(i=>!i.content.trim()||i.content.trim().length>200||/[\u0000-\u001f\u007f]/.test(i.content.trim()))){
      error='invalid';if(!title.trim())detailsOpen=true;return;
    }
    busy=true;error='';
    try {await session.save(title,note,items.map((i,n)=>({...i,content:i.content.trim(),sort_order:(n+1)*1000})));}
    catch(e){error=String(e).includes('STALE')?'conflict':'saveFailed';busy=false;return;}
    busy=false;onSaved();
  }
</script>

<dialog bind:this={dialog} aria-label={$translator('checklist.title')} onkeydown={e=>e.stopPropagation()}
  oncancel={e=>{e.preventDefault();if(!busy)onClose();}}>
  <form onsubmit={e=>{e.preventDefault();void save();}}>
    <header><h2>{$translator('checklist.title')}</h2>
      <span class="scope" title={$translator('checklist.currentOnly')}>{$translator('checklist.scope')}</span></header>
    <div class="fields">
      {#if loading}<p role="status">{$translator('common.loading')}</p>{/if}
      {#if error}<p role="alert">{$translator(`checklist.${error}`)}</p>{/if}
      {#if error==='loadFailed'}<button type="button" class="action-button" onclick={()=>void load()}>{$translator('common.retry')}</button>{/if}
      {#if loaded}
        <details class="task-details" bind:open={detailsOpen}>
          <summary><span>{title}</span><span class="edit-label">{$translator('common.edit')}</span></summary>
          <div class="task-fields">
            <label>{$translator('links.title')}<input bind:value={title} maxlength="100" disabled={busy||readOnly}/></label>
            <label>{$translator('todo.note')}<textarea bind:value={note} maxlength="1000" rows="2" disabled={busy||readOnly}></textarea></label>
          </div>
        </details>
        {#if readOnly}<p>{$translator('checklist.readOnly')}</p>{/if}
        <div class="section-head"><strong>{$translator('checklist.progress',{done,total:items.length})}</strong>
          <button type="button" class="action-button add-item" disabled={busy||readOnly||items.length>=20} onclick={add}>+ {$translator('checklist.add')}</button></div>
        <progress max={Math.max(1,items.length)} value={done} aria-label={$translator('checklist.progress',{done,total:items.length})}></progress>
        {#if !items.length}<p class="empty">{$translator('checklist.empty')}</p>{/if}
        <ol>
          {#each items as item,index (item.uuid)}
            <li class:completed={item.completed}>
              <label class="check-hit">
                <input type="checkbox" bind:checked={item.completed} disabled={busy||readOnly} aria-label={$translator('checklist.mark',{index:index+1})}/>
              </label>
              <textarea class="item-content" rows="1" use:fitText={item.content} bind:value={item.content} maxlength="200"
                disabled={busy||readOnly} aria-label={$translator('checklist.item',{index:index+1})}></textarea>
              <button type="button" class="item-more" disabled={busy||readOnly}
                title={$translator('common.more')} aria-label={$translator('common.more')} aria-expanded={expandedItem===item.uuid}
                onclick={()=>expandedItem=expandedItem===item.uuid?'':item.uuid}>&hellip;</button>
              {#if expandedItem===item.uuid}
                <div class="item-tools">
                  <button type="button" class="action-button" disabled={busy||readOnly||index===0} onclick={()=>move(index,-1)}>{$translator('checklist.up')}</button>
                  <button type="button" class="action-button" disabled={busy||readOnly||index===items.length-1} onclick={()=>move(index,1)}>{$translator('checklist.down')}</button>
                  <button type="button" class="action-button" data-tone="danger" disabled={busy||readOnly}
                    onclick={()=>{items=items.filter(i=>i.uuid!==item.uuid);expandedItem='';}}>{$translator('common.delete')}</button>
                </div>
              {/if}
            </li>
          {/each}
        </ol>
      {/if}
    </div>
    <footer>
      <button type="button" class="action-button" disabled={busy} onclick={onClose}>{$translator('common.cancel')}</button>
      <button type="submit" class="action-button" data-tone="primary" disabled={busy||loading||!loaded||readOnly}>{$translator('common.save')}</button>
    </footer>
  </form>
</dialog>
<style>
  dialog {
    --line:#e4dccb; --muted:#746853; --panel:#fffdf8; --field:#f4f0e7; --ink:#443d31;
    width:min(440px,calc(100% - 24px));max-height:calc(100% - 24px);box-sizing:border-box;
    padding:14px;border:1px solid var(--line);border-radius:8px;background:var(--panel);color:var(--ink);
    font-size:13px;line-height:1.45;
  }
  dialog[open]{display:flex;flex-direction:column;}
  dialog::backdrop{background:#0006;}
  form{display:flex;flex-direction:column;gap:12px;min-height:0;width:100%;}
  header{display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;}
  h2{font-size:15px;font-weight:600;margin:0;}
  .scope{font-size:11px;color:var(--muted);}
  .fields{min-height:0;overflow:auto;display:flex;flex-direction:column;gap:8px;scrollbar-width:thin;}
  .task-details{border-bottom:1px solid var(--line);padding-bottom:8px;}
  summary{display:flex;align-items:center;gap:8px;cursor:pointer;list-style:none;min-height:30px;}
  summary>span:first-child{flex:1;min-width:0;font-size:13px;font-weight:500;overflow-wrap:anywhere;}
  .edit-label{font-size:12px;color:var(--muted);flex-shrink:0;}
  .task-fields{display:grid;gap:8px;padding-top:8px;}
  label{display:grid;gap:4px;font-size:12px;}
  input:not([type=checkbox]),textarea{min-width:0;width:100%;box-sizing:border-box;padding:7px 8px;border:1px solid var(--line);
    border-radius:6px;font:inherit;color:inherit;background:var(--field);resize:none;}
  ol{list-style:none;margin:0;padding:0;}
  li{display:grid;grid-template-columns:32px minmax(0,1fr) 32px;gap:4px;align-items:start;padding:3px 0;border-bottom:1px solid var(--line);}
  li:last-child{border-bottom:0;}
  .check-hit{display:flex;align-items:center;justify-content:center;width:32px;min-height:32px;cursor:pointer;}
  input[type=checkbox]{width:16px;height:16px;margin:0;accent-color:#b28a19;}
  .item-content{border-color:transparent;background:transparent;line-height:1.45;font-size:13px;padding:6px 4px;min-height:32px;overflow:hidden;}
  .item-content:focus{outline:2px solid #c79b37;outline-offset:-2px;background:var(--field);}
  .completed .item-content{color:var(--muted);text-decoration:line-through;}
  .item-more{width:32px;min-height:32px;padding:0;border:0;border-radius:6px;background:transparent;color:var(--muted);font-size:18px;cursor:pointer;}
  .item-more:hover,.item-more[aria-expanded=true]{background:var(--field);}
  .item-more:focus-visible,summary:focus-visible{outline:2px solid #c79b37;outline-offset:-2px;}
  .item-tools{grid-column:2 / -1;display:flex;gap:6px;justify-content:flex-end;flex-wrap:wrap;padding:4px 0;}
  .item-tools button{font-size:12px;min-height:32px;}
  p{font-size:12px;line-height:1.5;margin:0;overflow-wrap:anywhere;}
  p[role=alert]{color:var(--ink);border-left:3px solid #bc713e;padding-left:10px;}
  .empty{padding:14px 0;color:var(--muted);text-align:center;}
  footer,.section-head{display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;}
  .section-head strong{font-size:12px;color:var(--muted);font-weight:500;}
  .add-item{font-size:12px;min-height:32px;}
  progress{display:block;width:100%;height:4px;flex-shrink:0;appearance:none;border:0;border-radius:2px;overflow:hidden;background:var(--field);color:#d6ad3a;}
  progress::-webkit-progress-bar{background:var(--field);}
  progress::-webkit-progress-value{background:#d6ad3a;}
  progress::-moz-progress-bar{background:#d6ad3a;}
  footer{justify-content:flex-end;border-top:1px solid var(--line);padding-top:10px;flex-shrink:0;}
  footer button{min-width:60px;min-height:32px;}
  @media (pointer:coarse) {
    li{grid-template-columns:44px minmax(0,1fr) 44px;}
    .check-hit,.item-more{width:44px;min-height:44px;}
    .item-content{min-height:44px;padding-top:11px;padding-bottom:11px;}
    .add-item,.item-tools button,footer button{min-height:44px;}
  }
  :global(html[data-theme=dark]) dialog{--panel:#29261f;--field:#363128;--line:#454034;--muted:#c2b59c;--ink:#f1e7d3;color-scheme:dark;}
</style>
