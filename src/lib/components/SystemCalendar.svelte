<script lang="ts">
  import { translator, languageState } from '$lib/i18n';
  import { formatDate, formatDateTime, formatTime } from '$lib/i18n/formatters';
  import { systemCalendar } from '$lib/stores/systemCalendarStore';
  import { calendarCoverageOnDate, calendarDayBounds, calendarFreshness,
    calendarOccurrencesOnDate, calendarOwnerLabel, calendarAllDayLastDate } from '$lib/utils/systemCalendarDates';
  import { calendarErrorKind } from '$lib/utils/systemCalendarErrors';
  import type { CalendarOccurrence } from '$lib/types/systemCalendar';

  export let dates: string[];
  export let now: number;

  $: document = $systemCalendar.document;
  $: freshness = calendarFreshness(document, $systemCalendar.last_received_at, now);
  $: days = dates.map(date => ({ date, coverage: calendarCoverageOnDate(document?.coverage ?? null, date),
    items: calendarOccurrencesOnDate(document, date) }));
  $: sourceNames = new Map(document?.calendars.map(source => [source.id, source.title]) ?? []);
  $: locale = $languageState.resolvedLocale;
  $: errorKind = calendarErrorKind($systemCalendar.error);

  function timeLabel(item: CalendarOccurrence) {
    if (item.isAllDay) return $translator('systemCalendar.allDay');
    const start = new Date(item.startTime);
    const end = new Date(item.endTime);
    if (item.startTime === item.endTime) return formatTime(start, locale);
    return start.toDateString() === end.toDateString()
      ? `${formatTime(start, locale)} - ${formatTime(end, locale)}`
      : `${formatDateTime(start, locale)} - ${formatDateTime(end, locale)}`;
  }
</script>

<section class="system-calendar" aria-label={$translator('systemCalendar.title')} aria-busy={$systemCalendar.loading}>
  <header>
    <div><h2>{$translator('systemCalendar.title')}</h2><span>{$translator('systemCalendar.readOnly')}</span></div>
    <button type="button" class="refresh" title={$translator('systemCalendar.refresh')}
      aria-label={$translator('systemCalendar.refresh')}
      disabled={$systemCalendar.loading || $systemCalendar.changingTarget || !$systemCalendar.configured}
      onclick={() => void systemCalendar.refresh()}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
        <path d="M20 7v5h-5M4 17v-5h5M6.1 7a7 7 0 0 1 11.6-2L20 8M4 16l2.3 3A7 7 0 0 0 18 17" />
      </svg>
    </button>
  </header>
  <div class="calendar-status" role="status">
    {#if $systemCalendar.changingTarget}
      <p>{$translator('systemCalendar.changingTarget')}</p>
    {:else if !$systemCalendar.configured && !$systemCalendar.error}
      <p>{$translator('systemCalendar.unconfigured')}</p>
    {:else}
      {#if $systemCalendar.loading}<p>{$translator('systemCalendar.loading')}</p>{/if}
      {#if document?.state === 'withdrawn'}
        <p>{$translator('systemCalendar.sharingOff')}</p>
      {:else if !document && !$systemCalendar.error && !$systemCalendar.loading && $systemCalendar.hydrated}
        <p>{$translator('systemCalendar.missing')}</p>
      {:else if !document && !$systemCalendar.hydrated && !$systemCalendar.loading}
        <p>{$translator('systemCalendar.notLoaded')}</p>
      {/if}
    {/if}
    {#if $systemCalendar.error && !$systemCalendar.changingTarget}
      <p class="warning">{$translator(`systemCalendar.error.${errorKind}`)}</p>
      {#if document?.state === 'active'}<p class="warning">{$translator('systemCalendar.cachedFailure')}</p>{/if}
    {/if}
  </div>
  {#if document?.state === 'active'}
    <details class="metadata-details">
      <summary>
        <span>{$translator('systemCalendar.source', { owner: calendarOwnerLabel(document.owner_id) })}</span>
        <span>{$translator('systemCalendar.captured', { time: formatDateTime(document.captured_at, locale) })}</span>
      </summary>
      <div class="metadata">
        <span>{$translator('systemCalendar.timezone', { zone: document.source_timezone })}</span>
        {#if $systemCalendar.last_received_at > 0}
          <span>{$translator('systemCalendar.received', { time: formatDateTime($systemCalendar.last_received_at, locale) })}</span>
        {/if}
        {#if document.coverage}
          <span>{$translator('systemCalendar.coverage', { start: document.coverage.start, end: document.coverage.end })}</span>
        {/if}
      </div>
    </details>
    {#if freshness.sourceStale}<p class="warning">{$translator('systemCalendar.sourceStale')}</p>{/if}
    <p class:warning={freshness.cacheStale} class="freshness">
      {$translator(freshness.cacheStale ? 'systemCalendar.cacheStale' : 'systemCalendar.cacheFresh')}
    </p>
    <div class="calendar-days">
      {#each days as day (day.date)}
        <section class="calendar-day" data-calendar-date={day.date}>
          <h3>{formatDate(calendarDayBounds(day.date)![0], { month: 'short', day: 'numeric', weekday: 'short' }, locale)}</h3>
          {#if day.coverage !== 'full'}
            <p class="warning">{$translator(day.coverage === 'outside' ? 'systemCalendar.outside' : 'systemCalendar.partial')}</p>
          {/if}
          {#if day.items.length === 0 && day.coverage === 'full'}
            <p>{$translator('systemCalendar.empty')}</p>
          {/if}
          <ul>
            {#each day.items as item (item.id)}
              <li>
                <span class="event-time">{timeLabel(item)}</span>
                <div class="event-content">
                  <strong>{item.title || $translator('systemCalendar.untitled')}</strong>
                  {#if item.isAllDay && calendarAllDayLastDate(item.endDateExclusive) !== item.startDate}
                    <span class="all-day-span">{$translator('systemCalendar.allDaySpan', {
                      start: item.startDate, end: calendarAllDayLastDate(item.endDateExclusive) })}</span>
                  {/if}
                  <span>{sourceNames.get(item.calendarId) || $translator('systemCalendar.unnamedSource')}</span>
                  {#if item.location}<span class="location">{item.location}</span>{/if}
                  {#if item.timeZone && item.timeZone !== document.source_timezone}
                    <span class="event-zone">{$translator('systemCalendar.eventTimezone', { zone: item.timeZone })}</span>
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
  {/if}
</section>

<style>
  .system-calendar { --calendar-muted: #686b65; --calendar-line: #d8dcd5; --calendar-accent: #316457;
    padding: 10px 0; border-block: 1px solid var(--calendar-line); margin: 4px 0 12px; font-size: 12px; min-width: 0; }
  header, header > div { display: flex; align-items: center; gap: 8px; }
  header { justify-content: space-between; }
  header > div { flex-wrap: wrap; min-width: 0; }
  h2 { font-size: 13px; margin: 0; }
  header span, .metadata-details, .freshness, .event-content > span { color: var(--calendar-muted); }
  .refresh { flex: 0 0 30px; height: 30px; display: grid; place-items: center; border: 1px solid var(--calendar-line);
    border-radius: 6px; background: transparent; cursor: pointer; }
  .refresh:disabled { opacity: .5; cursor: default; }
  .refresh:focus-visible, summary:focus-visible { outline: 2px solid var(--calendar-accent); outline-offset: 2px; }
  p { margin: 5px 0; overflow-wrap: anywhere; }
  .metadata { display: flex; flex-wrap: wrap; gap: 3px 12px; font-size: 11px; }
  .metadata span { overflow-wrap: anywhere; }
  .metadata-details { font-size: 11px; margin-top: 4px; }
  summary { cursor: pointer; overflow-wrap: anywhere; }
  summary span:last-child { display: block; margin-top: 3px; }
  .warning { color: #9c432b; }
  .freshness { font-size: 11px; }
  .calendar-days { max-height: 280px; overflow: auto; scrollbar-gutter: stable; }
  h3 { font-size: 12px; margin: 9px 0 4px; color: var(--calendar-accent); }
  ul { list-style: none; padding: 0; margin: 0; }
  li { display: grid; grid-template-columns: minmax(60px, 30%) minmax(0, 1fr); gap: 10px;
    padding: 7px 0; border-bottom: 1px solid var(--calendar-line); }
  .event-time { font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
  .event-content { display: grid; gap: 3px; min-width: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .event-content > span { font-size: 11px; }
  :global(html[data-theme='dark']) .system-calendar { --calendar-muted: #b7c1b7; --calendar-line: #50574f; --calendar-accent: #a4d4bd; }
  :global(html[data-theme='dark']) .warning { color: #f4ad97; }
</style>
