<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { isTauri } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { languageState, translator, type ResolvedLocale } from '$lib/i18n';
  import { todos } from '$lib/stores/todoStore';
  import { savedSyncSettings } from '$lib/sync/autoSync';
  import { systemCalendar } from '$lib/stores/systemCalendarStore';
  import { taskProgressCounts, watchTaskProgress } from '$lib/stores/taskProgressCounts';
  import { createWorkReviewStore } from '$lib/stores/workReviewStore';
  import { reviewDateKey, reviewDateRange, reviewPeriodDates, type ReviewPeriod } from '$lib/utils/workReview';
  import type { ReviewRow } from '$lib/types/workReview';
  import type { SearchTarget } from '$lib/api/contentSearchApi';
  import TaskProgressDialog from './TaskProgressDialog.svelte';

  export let onClose: () => void;
  const review = createWorkReviewStore();
  let period: ReviewPeriod = 'week', dates = reviewPeriodDates('week'), group = 'all', keyword = '';
  let dialog: HTMLDialogElement, content: HTMLDivElement;
  let preview: SearchTarget | null = null, expanded = new Set<string>();
  let mounted = false, timer: ReturnType<typeof setTimeout> | null = null;
  let scrollTop = 0;
  let viewVersion = 0;
  let reloadPosition = 0;
  let compact = false;
  let zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  let offset = new Date().getTimezoneOffset();
  $: range = reviewDateRange(dates);
  $: groupName = group === 'all' ? $translator('workReview.allGroups') : group === 'ungrouped' ? $translator('workReview.ungrouped') : $todos.groups.find(item => item.uuid === group)?.name ?? $translator('workReview.ungrouped');
  $: dateFormat = new Intl.DateTimeFormat($languageState.resolvedLocale, { dateStyle: 'medium' });
  $: timeFormat = new Intl.DateTimeFormat($languageState.resolvedLocale, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' });
  $: if (mounted && group !== 'all' && group !== 'ungrouped' && !$todos.groups.some(item => item.uuid === group)) { group = 'all'; change(); }

  function change(debounce = false) {
    viewVersion++;
    reloadPosition = 0;
    if (timer) clearTimeout(timer);
    expanded = new Set(); scrollTop = 0;
    if (content) content.scrollTop = 0;
    const current = reviewDateRange(dates);
    review.setQuery(current ? { ...current, group_scope: group === 'all' ? 'all' : group === 'ungrouped' ? 'ungrouped' : 'group',
      group_uuid: group === 'all' || group === 'ungrouped' ? null : group, keyword } : null);
    timer = setTimeout(() => { timer = null; void review.refresh(); }, debounce ? 250 : 0);
  }
  async function reload() {
    if (!$review.loading) reloadPosition = preview ? scrollTop : content?.scrollTop ?? 0;
    const position = reloadPosition, version = viewVersion;
    await review.invalidate(); await tick();
    if (mounted && version === viewVersion) content.scrollTop = position;
  }
  function setPeriod(value: ReviewPeriod) {
    period = value;
    if (period !== 'custom') dates = reviewPeriodDates(period);
    change();
  }
  function checkTime() {
    const nextZone = Intl.DateTimeFormat().resolvedOptions().timeZone, nextOffset = new Date().getTimezoneOffset();
    const nextDates = period === 'custom' ? dates : reviewPeriodDates(period);
    if (nextZone !== zone || nextOffset !== offset || nextDates.start !== dates.start || nextDates.end !== dates.end) {
      zone = nextZone; offset = nextOffset; dates = nextDates; change();
      // Equal UTC ranges still need to invalidate a summary with different local dates.
      void review.invalidate();
    }
  }
  async function select(row: ReviewRow) {
    scrollTop = content.scrollTop;
    const result = await review.open(row);
    if (mounted && result) preview = result;
  }
  async function back() {
    preview = null;
    await tick();
    if (mounted) { content.scrollTop = scrollTop; dialog.focus(); }
  }
  function toggle(uuid: string) {
    const next = new Set(expanded);
    if (next.has(uuid)) next.delete(uuid); else next.add(uuid);
    expanded = next;
  }
  function fitDialog() {
    const scale = dialog.getBoundingClientRect().width / dialog.offsetWidth || 1;
    const available = (window.innerHeight - 24) / scale;
    compact = available < 430;
    dialog.style.height = `${Math.min(740, available)}px`;
    dialog.style.maxHeight = `${available}px`;
    dialog.style.maxWidth = `${(window.innerWidth - 24) / scale}px`;
  }
  onMount(() => {
    mounted = true; dialog.showModal(); fitDialog(); change();
    const stops: (() => void)[] = [];
    let items = $todos.items, groups = $todos.groups, locale: ResolvedLocale = $languageState.resolvedLocale;
    let identity = '', blocked = false;
    const updateTarget = () => review.setTarget(identity, blocked);
    stops.push(todos.subscribe(value => {
      if (items !== value.items || groups !== value.groups) { items = value.items; groups = value.groups; void reload(); }
    }));
    stops.push(savedSyncSettings.subscribe(value => {
      identity = value ? JSON.stringify([value.enabled, value.endpoint, value.bucket, value.objectKey, value.region, value.pathStyle, value.allowHttp, value.credentialsConfigured]) : '';
      updateTarget();
    }));
    stops.push(systemCalendar.subscribe(value => { if (blocked !== value.changingTarget) { blocked = value.changingTarget; updateTarget(); } }));
    stops.push(languageState.subscribe(value => { if (locale !== value.resolvedLocale) { locale = value.resolvedLocale; void reload(); } }));
    stops.push(watchTaskProgress(), taskProgressCounts.onChanged(() => void reload()));
    if (isTauri()) {
      // Also cover archived tasks not represented in the main todo store.
      for (const event of ['todos-changed', 'system-calendar-state-changed']) {
        void listen(event, () => void reload()).then(stop => { if (mounted) stops.push(stop); else stop(); }).catch(() => {});
      }
    }
    const interval = setInterval(checkTime, 30000);
    window.addEventListener('focus', checkTime);
    window.addEventListener('resize', fitDialog);
    const zoomObserver = new MutationObserver(fitDialog);
    zoomObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['style'] });
    return () => { mounted = false; if (timer) clearTimeout(timer); clearInterval(interval); window.removeEventListener('focus', checkTime);
      window.removeEventListener('resize', fitDialog); zoomObserver.disconnect();
      stops.forEach(stop => stop()); review.dispose(); dialog.close(); };
  });
</script>

<dialog class="work-review-dialog" bind:this={dialog} class:compact tabindex="-1" aria-labelledby="work-review-title"
  onkeydown={event => event.stopPropagation()} oncancel={event => { event.preventDefault(); if (!preview) onClose(); }}>
  <header><h2 id="work-review-title">{$translator('workReview.title')}</h2>
    <button class="action-button" title={$translator('common.close')} aria-label={$translator('common.close')} onclick={onClose}>×</button>
  </header>
  <div class="filters">
    {#if compact}
      <select aria-label={$translator('workReview.period')} title={`${dates.start} - ${dates.end}`} value={period} onchange={event => setPeriod(event.currentTarget.value as ReviewPeriod)}>
        {#each ['today', 'week', 'previousWeek', 'custom'] as choice}<option value={choice}>{$translator(`workReview.${choice as ReviewPeriod}`)}</option>{/each}
      </select>
    {:else}
    <div class="periods" role="group" aria-label={$translator('workReview.period')}>
      {#each ['today', 'week', 'previousWeek', 'custom'] as choice}
        <button class="action-button" data-tone={period === choice ? 'primary' : undefined} aria-pressed={period === choice}
          onclick={() => setPeriod(choice as ReviewPeriod)}>{$translator(`workReview.${choice as ReviewPeriod}`)}</button>
      {/each}
    </div>
    {/if}
    {#if period === 'custom'}
      <div class="date-inputs">
        <label>{$translator('workReview.start')}<input type="date" min="1970-01-01" value={dates.start} oninput={event => { dates = { ...dates, start: event.currentTarget.value }; change(); }} /></label>
        <label>{$translator('workReview.end')}<input type="date" min="1970-01-01" value={dates.end} oninput={event => { dates = { ...dates, end: event.currentTarget.value }; change(); }} /></label>
      </div>
    {:else if !compact}<p class="range">{dates.start} – {dates.end}</p>{/if}
    <div class="query-inputs">
      <select aria-label={$translator('workReview.group')} value={group} onchange={event => { group = event.currentTarget.value; change(); }}>
        <option value="all">{$translator('workReview.allGroups')}</option><option value="ungrouped">{$translator('workReview.ungrouped')}</option>
        {#each $todos.groups as item (item.uuid)}<option value={item.uuid}>{item.name}</option>{/each}
      </select>
      <input type="search" maxlength="100" value={keyword} aria-label={$translator('workReview.keyword')} placeholder={$translator('workReview.keyword')} oninput={event => { keyword = event.currentTarget.value; change(true); }} />
    </div>
  </div>
  {#if !range}<p class="feedback" role="alert">{$translator('workReview.error.invalid')}</p>{/if}
  {#if $review.blocked}<p class="feedback" role="status">{$translator('workReview.changingTarget')}</p>{/if}
  {#if $review.ready}<p class="counts">{$translator('workReview.counts', { tasks: $review.tasks, entries: $review.entries })}</p>{/if}
  <div class="entries" bind:this={content} aria-busy={$review.loading || $review.opening}>
    {#if $review.loading || $review.opening}<p role="status">{$translator('common.loading')}</p>{/if}
    {#if $review.ready && !$review.rows.length}<p class="empty">{$translator(keyword.trim() || group !== 'all' ? 'workReview.noMatches' : 'workReview.empty')}</p>{/if}
    {#each $review.rows as row, index (row.record_uuid)}
      {#if index === 0 || reviewDateKey(row.created_at) !== reviewDateKey($review.rows[index - 1].created_at)}<h3>{dateFormat.format(row.created_at)}</h3>{/if}
      <article>
        <div class="meta"><time datetime={new Date(row.created_at).toISOString()}>{timeFormat.format(row.created_at)}</time>
          {#if row.group_name}<span>{row.group_name}</span>{/if}
          <span>{$translator(row.completed ? 'contentSearch.completed' : 'contentSearch.incomplete')}</span>
          {#if row.archived}<span>{$translator('contentSearch.archived')}</span>{/if}
          {#if row.updated_at > row.created_at}<span title={dateFormat.format(row.updated_at) + ' ' + timeFormat.format(row.updated_at)}>{$translator('taskProgress.edited')}</span>{/if}
        </div>
        <button class="title" disabled={$review.loading || $review.opening || $review.blocked} onclick={() => select(row)}>{row.task_title}</button>
        <button class="body" class:collapsed={!expanded.has(row.record_uuid)} disabled={$review.loading || $review.opening || $review.blocked} onclick={() => select(row)}>{row.body}</button>
        {#if row.body.length > 180 || row.body.split('\n').length > 3}
          <button class="expand" aria-expanded={expanded.has(row.record_uuid)} onclick={() => toggle(row.record_uuid)}>{$translator(expanded.has(row.record_uuid) ? 'workReview.collapse' : 'workReview.expand')}</button>
        {/if}
      </article>
    {/each}
    {#if $review.cursor}<button class="action-button" disabled={$review.loading || $review.opening || $review.blocked} onclick={() => review.loadMore()}>{$translator('taskProgress.more')}</button>{/if}
  </div>
  <footer>
    {#if $review.error && ($review.error !== 'invalid' || range)}<p class="feedback" role="alert">{$translator(`workReview.error.${$review.error}`)}</p>{/if}
    {#if $review.copied}<p class="feedback" role="status">{$translator('workReview.copied')}</p>{/if}
    <div class="footer-actions">
      {#if $review.error}<button class="action-button" disabled={$review.loading || $review.blocked || !range} onclick={() => review.refresh()}>{$translator('common.retry')}</button>{/if}
      <button class="action-button" data-tone="primary" disabled={!$review.ready || !$review.entries || $review.loading || $review.copying || $review.blocked || !range}
        onclick={() => review.copy({ dates, groupName, locale: $languageState.resolvedLocale })}>{$translator($review.copying ? 'workReview.copying' : 'workReview.copy')}</button>
    </div>
  </footer>
</dialog>
{#if preview}<TaskProgressDialog uuid={preview.uuid} title={preview.title} readOnly={preview.archived} onClose={() => void back()} />{/if}

<style>
  dialog { --review-bg: #fffdf8; width: min(700px, calc(100% - 24px)); height: min(740px, calc(100% - 24px)); max-width: calc(100% - 24px); max-height: calc(100% - 24px); box-sizing: border-box; padding: 14px; border: 1px solid var(--action-border); border-radius: 8px; background: var(--review-bg); color: var(--action-text); font-size: 13px; line-height: 1.5; }
  :global(html[data-theme='dark']) dialog { --review-bg: #29251e; color-scheme: dark; }
  dialog[open] { display: flex; flex-direction: column; gap: 10px; } dialog::backdrop { background: #0006; }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 8px; flex: none; }
  h2 { margin: 0; font-size: 17px; overflow-wrap: anywhere; } h3 { font-size: 12px; margin: 12px 0 4px; }
  header button { flex: none; } .filters, footer, .counts, .feedback { flex: none; }
  .periods, .query-inputs, .date-inputs { display: flex; flex-wrap: wrap; gap: 6px; }
  .periods .action-button { flex: 1 1 auto; } .query-inputs { margin-top: 8px; }
  .query-inputs input { flex: 2 1 180px; } select { flex: 1 1 120px; }
  .date-inputs { margin-top: 8px; } label { flex: 1 1 145px; min-width: 0; font-size: 12px; }
  input, select { box-sizing: border-box; min-width: 0; max-width: 100%; width: 100%; padding: 7px; border: 1px solid var(--action-border); border-radius: 6px; background: var(--review-bg); color: inherit; font: inherit; }
  input:focus-visible, select:focus-visible, .title:focus-visible, .body:focus-visible, .expand:focus-visible { outline: 2px solid var(--action-focus); outline-offset: 1px; }
  input::placeholder { color: inherit; opacity: .75; }
  .range, .counts, .feedback { margin: 0; overflow-wrap: anywhere; } .range { margin-top: 6px; font-size: 12px; }
  .entries { flex: 1; min-height: 60px; overflow: auto; scrollbar-width: thin; scrollbar-gutter: stable; }
  article { padding: 8px 0; border-bottom: 1px solid var(--action-border); }
  .meta { display: flex; flex-wrap: wrap; gap: 4px 8px; font-size: 11px; overflow-wrap: anywhere; }
  .meta span { max-width: 100%; } .title, .body, .expand { display: block; border: 0; background: transparent; color: inherit; padding: 0; font: inherit; text-align: left; cursor: pointer; }
  .title, .body { width: 100%; white-space: pre-wrap; overflow-wrap: anywhere; margin-top: 4px; } .title { font-weight: 600; }
  .body.collapsed { display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 3; line-clamp: 3; overflow: hidden; }
  .expand { font-size: 12px; margin-top: 4px; text-decoration: underline; min-height: 28px; }
  footer { border-top: 1px solid var(--action-border); padding-top: 10px; } .footer-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 6px; }
  @media (max-width: 400px) { dialog { padding: 10px; } .periods .action-button { flex: 1 1 40%; } }
  dialog.compact { gap: 6px; } .compact .filters { max-height: 38%; min-height: 0; flex-shrink: 1; overflow: auto; scrollbar-width: thin; }
  .compact h3 { margin: 4px 0; }
  .compact .entries { min-height: 30px; } .compact footer { padding-top: 4px; }
  .compact .footer-actions { margin-top: 0; }
</style>
