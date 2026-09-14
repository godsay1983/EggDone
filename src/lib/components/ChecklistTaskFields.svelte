<script lang="ts">
  import { translator } from '$lib/i18n';
  import type { TaskEditorFields } from '$lib/types/taskChecklistEditor';
  import type { TodoGroup } from '$lib/types';
  import { timestampToDateTimeLocal, dateTimeLocalToTimestamp } from '$lib/utils/reminderTimes';
  export let fields: TaskEditorFields;
  export let choice = 'keep';
  export let groups: TodoGroup[] = [];
  export let disabled = false;
  export let repeatEnabled = false;
  export let onCustom: () => void;
  export let customSummary = '';
  export let repeatSummary = '';
  function dateValue(): string { return fields.due_date ?? (fields.due_at === null ? '' : timestampToDateTimeLocal(fields.due_at).slice(0,10)); }
  function setDate(day: string) {
    const time=fields.due_at===null?'':timestampToDateTimeLocal(fields.due_at).slice(11,16);
    fields = {...fields, due_date:day && !time ? day : null, due_at:day && time ? dateTimeLocalToTimestamp(day+'T'+time) : null};
  }
  function setTime(time: string) {
    const day=dateValue();
    fields={...fields,due_date:time ? null : day || null,due_at:time && day ? dateTimeLocalToTimestamp(day+'T'+time) : null};
  }
</script>
<details class="task-settings">
 <summary>{$translator('checklist.settings')}</summary>
 <fieldset {disabled}>
  <label>{$translator('checklist.date')}<input aria-label={$translator('checklist.date')} type="date" min="1900-01-01" max="9999-12-31" value={dateValue()} onchange={e=>setDate(e.currentTarget.value)}/></label>
  <label>{$translator('checklist.time')}<input aria-label={$translator('checklist.time')} type="time" disabled={!dateValue()} value={fields.due_at===null?'':timestampToDateTimeLocal(fields.due_at).slice(11,16)} onchange={e=>setTime(e.currentTarget.value)}/></label>
  <label>{$translator('todo.reminder')}<input aria-label={$translator('todo.reminder')} type="datetime-local" value={fields.reminder_at===null?'':timestampToDateTimeLocal(fields.reminder_at)} onchange={e=>fields={...fields,reminder_at:dateTimeLocalToTimestamp(e.currentTarget.value)}}/></label>
  <label>{$translator('todo.group')}<select aria-label={$translator('todo.group')} value={fields.group_uuid??''} onchange={e=>fields={...fields,group_uuid:e.currentTarget.value||null}}>
   <option value="">{$translator('todo.noGroup')}</option>
   {#if fields.group_uuid && !groups.some(g=>g.uuid===fields.group_uuid)}<option value={fields.group_uuid}>{$translator('checklist.missingGroup')}</option>{/if}
   {#each groups as group}<option value={group.uuid}>{group.name}</option>{/each}
  </select></label>
  <label class="important"><input type="checkbox" checked={fields.priority>0} onchange={e=>fields={...fields,priority:e.currentTarget.checked?1:0}}/>{$translator('checklist.important')}</label>
  <label>{$translator('todo.repeat')}<select aria-label={$translator('todo.repeat')} bind:value={choice} disabled={!repeatEnabled}>
   <option value="keep">{$translator('checklist.keepRepeat')}</option>
   <option value="none">{$translator('todo.noRepeat')}</option>
   <option value="daily">{$translator('recurrence.daily')}</option>
   <option value="weekly">{$translator('recurrence.weekly')}</option>
   <option value="monthly">{$translator('recurrence.monthly')}</option>
   <option value="weekdays">{$translator('checklist.weekdays')}</option>
   <option value="custom">{$translator('recurrence.title')}</option>
  </select></label>
  {#if choice==='keep' && repeatSummary}<p>{repeatSummary}</p>{/if}
  {#if choice==='custom'}<button type="button" onclick={onCustom}>{$translator('checklist.configureRule')}</button>{#if customSummary}<p>{customSummary}</p>{/if}{/if}
  {#if choice!=='keep'}<p>{$translator(choice==='none'?'checklist.stopNotice':'checklist.ruleChangeNotice')}</p>{/if}
 </fieldset>
</details>
<style>
 summary{cursor:pointer;font-size:12px;color:var(--muted);padding:8px 0;}
 fieldset{border:0;margin:0;padding:0;min-width:0;display:grid;gap:8px;}
 label{display:grid;gap:4px;font-size:12px;min-width:0;}
 input:not([type=checkbox]),select{width:100%;min-width:0;box-sizing:border-box;font:inherit;color:inherit;background:var(--field);border:1px solid var(--line);border-radius:6px;padding:6px;}
 :global(html[data-theme=dark]) input,:global(html[data-theme=dark]) select{color-scheme:dark;}
 .important{display:flex;align-items:center;gap:6px;} .important input{margin:0;accent-color:#b28a19;}
 button{justify-self:start;min-height:32px;padding:4px 10px;font:inherit;font-size:12px;color:inherit;background:var(--field);border:1px solid var(--line);border-radius:6px;cursor:pointer;}
 p{font-size:12px;color:var(--muted);margin:0;overflow-wrap:anywhere;}
 :disabled{opacity:.55;}
</style>
