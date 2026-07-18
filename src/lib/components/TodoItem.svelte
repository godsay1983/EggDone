<script lang="ts">
  import { onMount, tick } from "svelte";
  import { fly } from "svelte/transition";

  import { languageState, translator } from "$lib/i18n";
  import type { TodoScheduleInput } from "$lib/api/todoApi";
  import type {
    RepeatDeleteScope,
    RepeatEditScope,
    RepeatRule,
    Todo,
    TodoGroup,
  } from "$lib/types";
  import {
    formatDueLabel,
    getDueTone,
    localDateString,
  } from "$lib/utils/todoDates";
  import {
    defaultCustomReminderDateTime,
    dateTimeLocalToTimestamp,
    inferReminderChoice,
    laterTodayReminderAt,
    reminderAtForDate,
    snoozeReminderAt,
    timestampToDateTimeLocal,
    type ReminderChoice,
  } from "$lib/utils/reminderTimes";

  export let todo: Todo;
  export let onToggle: (todo: Todo) => Promise<void>;
  export let onEdit: (
    id: number,
    title: string,
    repeatScope?: RepeatEditScope,
  ) => Promise<void>;
  export let onNote: (
    id: number,
    note: string | null,
    repeatScope?: RepeatEditScope,
  ) => Promise<void>;
  export let onPin: (todo: Todo, pinned: boolean) => Promise<void>;
  export let onPriority: (todo: Todo, priority: number) => Promise<void>;
  export let onFocus: (todo: Todo) => void;
  export let onSchedule: (
    id: number,
    schedule: TodoScheduleInput,
    repeatScope?: RepeatEditScope,
  ) => Promise<void>;
  export let onSnooze: (todo: Todo, reminderAt: number) => Promise<void>;
  export let groups: TodoGroup[] = [];
  export let onGroupChange: (
    todo: Todo,
    groupUuid: string | null,
    repeatScope?: RepeatEditScope,
  ) => Promise<void>;
  export let onDelete: (
    id: number,
    repeatScope?: RepeatDeleteScope,
  ) => Promise<void>;
  export let onMove: (todo: Todo, direction: -1 | 1) => Promise<void>;
  export let onDragStart: (todo: Todo, event: PointerEvent) => void;
  export let batchMode = false;
  export let batchSelected = false;
  export let onBatchSelect: (todo: Todo, selected: boolean) => void;
  export let canMoveUp = false;
  export let canMoveDown = false;
  export let isDragging = false;
  export let isDragTarget = false;
  export let dragDisabled = false;
  export let reorderDisabled = false;
  export let editRequest = 0;

  let editing = false;
  let editTitle = "";
  let editError = "";
  let saving = false;
  let scheduleOpen = false;
  let scheduleSaving = false;
  let scheduleError = "";
  let noteOpen = false;
  let noteDraft = "";
  let noteSaving = false;
  let noteError = "";
  let customDate = "";
  let customDueTime = "18:00";
  let customReminderDateTime = "";
  let reminderChoice: ReminderChoice = "none";
  let repeatChoice: RepeatRule | "none" = "none";
  let groupSaving = false;
  let actionsOpen = false;
  let editInput: HTMLInputElement;
  let noteInput: HTMLTextAreaElement;
  let itemElement: HTMLElement;
  let animationDuration = 140;
  $: dueLabel = localizedDueLabel(todo);
  $: dueTone = getDueTone(todo);
  $: notePreview = todo.note?.trim() ?? "";
  $: currentGroup =
    todo.group_uuid === null
      ? undefined
      : groups.find((group) => group.uuid === todo.group_uuid);
  $: currentGroupColor = groupColorValue(currentGroup?.color);
  $: canSaveSchedule =
    Boolean(customDate) &&
    dateTimeLocalToTimestamp(`${customDate}T${customDueTime}`) !== null &&
    (reminderChoice !== "custom" || Boolean(customReminderDateTime));
  $: if (editRequest > 0 && !editing) {
    void beginEdit();
  }

  onMount(() => {
    animationDuration = window.matchMedia("(prefers-reduced-motion: reduce)").matches
      ? 0
      : 140;

    function handlePointerDown(event: PointerEvent) {
      if (
        editing &&
        event.target instanceof Node &&
        !itemElement.contains(event.target)
      ) {
        void saveEdit();
      }
      if (
        scheduleOpen &&
        event.target instanceof Node &&
        !itemElement.contains(event.target)
      ) {
        scheduleOpen = false;
      }
      if (
        noteOpen &&
        event.target instanceof Node &&
        !itemElement.contains(event.target)
      ) {
        noteOpen = false;
        noteError = "";
      }
      if (
        actionsOpen &&
        event.target instanceof Node &&
        !itemElement.contains(event.target)
      ) {
        actionsOpen = false;
      }
    }

    window.addEventListener("pointerdown", handlePointerDown, true);
    return () => window.removeEventListener("pointerdown", handlePointerDown, true);
  });

  async function beginEdit() {
    scheduleOpen = false;
    noteOpen = false;
    actionsOpen = false;
    editing = true;
    editTitle = todo.title;
    editError = "";
    await tick();
    editInput?.focus();
    editInput?.select();
  }

  function cancelEdit() {
    editing = false;
    editTitle = todo.title;
    editError = "";
  }

  function toggleSchedule() {
    if (editing) return;
    actionsOpen = false;
    noteOpen = false;
    scheduleOpen = !scheduleOpen;
    scheduleError = "";
    const dueDateTime =
      todo.due_at !== null ? timestampToDateTimeLocal(todo.due_at) : null;
    customDate = todo.due_date ?? dueDateTime?.slice(0, 10) ?? localDateString(0);
    customDueTime = dueDateTime?.slice(11, 16) ?? "18:00";
    reminderChoice = inferReminderChoice(customDate, todo.reminder_at);
    repeatChoice = todo.repeat_rule ?? "none";
    customReminderDateTime =
      todo.reminder_at !== null
        ? timestampToDateTimeLocal(todo.reminder_at)
        : defaultCustomReminderDateTime(customDate);
  }

  function toggleActions() {
    if (editing) return;
    scheduleOpen = false;
    noteOpen = false;
    actionsOpen = !actionsOpen;
  }

  async function openNoteEditor() {
    if (editing) return;
    scheduleOpen = false;
    actionsOpen = false;
    noteOpen = true;
    noteDraft = todo.note ?? "";
    noteError = "";
    await tick();
    noteInput?.focus();
  }

  async function setSchedule(date: string | null) {
    if (scheduleSaving) return;
    const dueAt =
      date === null
        ? null
        : dateTimeLocalToTimestamp(`${date}T${customDueTime}`);
    if (date !== null && dueAt === null) {
      scheduleError = $translator("todo.invalidDue");
      return;
    }
    const repeatRule = date && repeatChoice !== "none" ? repeatChoice : null;
    const reminderAt = date
      ? reminderAtForDate(date, reminderChoice, customReminderDateTime)
      : null;
    if (date && reminderChoice === "custom" && reminderAt === null) {
      scheduleError = $translator("todo.invalidReminder");
      return;
    }

    scheduleSaving = true;
    scheduleError = "";
    try {
      await onSchedule(
        todo.id,
        {
          due_date: null,
          due_at: dueAt,
          reminder_at: reminderAt,
          repeat_rule: repeatRule,
        },
        chooseRepeatEditScope($translator("todo.schedule")),
      );
      scheduleOpen = false;
    } catch {
      scheduleError = $translator("todo.scheduleSaveFailed");
    } finally {
      scheduleSaving = false;
    }
  }

  function handleReminderChoiceChange() {
    if (reminderChoice === "custom" && !customReminderDateTime) {
      customReminderDateTime = defaultCustomReminderDateTime(customDate || localDateString(0));
    }
  }

  function handleCustomDateChange() {
    if (reminderChoice !== "custom" || !customDate) return;
    const time = customReminderDateTime.split("T")[1] || "09:00";
    customReminderDateTime = `${customDate}T${time}`;
  }

  function formatReminderLabel(reminderAt: number | null) {
    if (reminderAt === null) return "";
    return new Intl.DateTimeFormat($languageState.resolvedLocale, {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(reminderAt));
  }

  function repeatLabel(rule: RepeatRule | null) {
    if (rule === "daily") return $translator("todo.repeatDaily");
    if (rule === "weekly") return $translator("todo.repeatWeekly");
    if (rule === "monthly") return $translator("todo.repeatMonthly");
    if (rule === "weekdays") return $translator("todo.repeatWeekdays");
    return "";
  }

  function localizedDueLabel(value: Todo) {
    const label = formatDueLabel(value);
    if (label === "今天") return $translator("todo.today");
    if (label === "明天") return $translator("todo.tomorrow");
    if (label.startsWith("今天 ")) return `${$translator("todo.today")} ${label.slice(3)}`;
    if (label.startsWith("明天 ")) return `${$translator("todo.tomorrow")} ${label.slice(3)}`;
    return label;
  }

  function groupColorValue(color: string | undefined) {
    if (
      color === "green" ||
      color === "blue" ||
      color === "peach" ||
      color === "lavender" ||
      color === "gray"
    ) {
      return color;
    }
    return "yellow";
  }

  function chooseRepeatEditScope(action: string): RepeatEditScope {
    if (todo.repeat_rule === null) return "single";
    return window.confirm(
      $translator("todo.repeatScopePrompt", { action }),
    )
      ? "future"
      : "single";
  }

  async function moveToGroup(value: string) {
    if (groupSaving) return;
    groupSaving = true;
    try {
      await onGroupChange(
        todo,
        value === "ungrouped" ? null : value,
        chooseRepeatEditScope($translator("todo.moveTo")),
      );
      actionsOpen = false;
    } finally {
      groupSaving = false;
    }
  }

  async function saveEdit() {
    if (saving) return;

    const nextTitle = editTitle.trim();
    if (!nextTitle) {
      editError = $translator("todo.emptyTitle");
      return;
    }
    if (nextTitle === todo.title) {
      cancelEdit();
      return;
    }

    saving = true;
    editError = "";
    try {
      await onEdit(todo.id, nextTitle, chooseRepeatEditScope($translator("common.edit")));
      editing = false;
    } catch {
      editError = $translator("todo.editFailed");
      await tick();
      editInput?.focus();
    } finally {
      saving = false;
    }
  }

  async function saveNote() {
    if (noteSaving) return;

    const nextNote = noteDraft.trim();
    const normalizedNote = nextNote ? nextNote : null;
    if (normalizedNote === todo.note) {
      noteOpen = false;
      noteError = "";
      return;
    }

    noteSaving = true;
    noteError = "";
    try {
      await onNote(todo.id, normalizedNote, chooseRepeatEditScope($translator("todo.editNote")));
      noteOpen = false;
    } catch {
      noteError = $translator("todo.noteSaveFailed");
      await tick();
      noteInput?.focus();
    } finally {
      noteSaving = false;
    }
  }

  function handleEditKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      void saveEdit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelEdit();
    }
  }

  function handleNoteKeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault();
      void saveNote();
    } else if (event.key === "Escape") {
      event.preventDefault();
      noteOpen = false;
      noteError = "";
    }
  }
</script>

<article
  bind:this={itemElement}
  class:completed={todo.completed}
  class:editing
  class:dragging={isDragging}
  class:drag-target={isDragTarget}
  class="todo-item"
  data-todo-id={todo.id}
  ondblclick={() => void beginEdit()}
  oncontextmenu={(event) => {
    event.preventDefault();
    void beginEdit();
  }}
  in:fly={{ y: -6, duration: animationDuration }}
  out:fly={{ x: 12, duration: animationDuration }}
>
  {#if batchMode}
    <button
      class:checked={batchSelected}
      class="batch-select"
      type="button"
      aria-label={batchSelected ? $translator("todo.unselect", { title: todo.title }) : $translator("todo.select", { title: todo.title })}
      aria-pressed={batchSelected}
      onclick={(event) => {
        event.stopPropagation();
        onBatchSelect(todo, !batchSelected);
      }}
    >
      {#if batchSelected}
        <svg viewBox="0 0 20 20" aria-hidden="true">
          <path d="m4 10 4 4 8-9" />
        </svg>
      {/if}
    </button>
  {/if}

  <button
    class="drag-handle"
    type="button"
    aria-label={$translator("todo.dragToSortOrGroup")}
    title={
      dragDisabled
        ? $translator("todo.dragDisabled")
        : reorderDisabled
          ? $translator("todo.dragToGroup")
          : $translator("todo.dragToSortOrGroup")
    }
    disabled={dragDisabled}
    onpointerdown={(event) => {
      if (event.button !== 0 || dragDisabled) return;
      event.preventDefault();
      event.stopPropagation();
      onDragStart(todo, event);
    }}
  >
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <circle cx="7" cy="6" r="1" />
      <circle cx="13" cy="6" r="1" />
      <circle cx="7" cy="10" r="1" />
      <circle cx="13" cy="10" r="1" />
      <circle cx="7" cy="14" r="1" />
      <circle cx="13" cy="14" r="1" />
    </svg>
  </button>

  <button
    class="checkbox"
    class:checked={todo.completed}
    type="button"
    aria-label={todo.completed ? $translator("todo.markIncomplete") : $translator("todo.markCompleted")}
    onclick={() => void onToggle(todo)}
    disabled={editing}
  >
    {#if todo.completed}
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <path d="m4 10 4 4 8-9" />
      </svg>
    {/if}
  </button>

  {#if editing}
    <div class="edit-area">
      <input
        bind:this={editInput}
        bind:value={editTitle}
        maxlength="200"
        aria-label={$translator("todo.editTitle", { title: todo.title })}
        disabled={saving}
        onkeydown={handleEditKeydown}
      />
      {#if editError}<small>{editError}</small>{/if}
    </div>
  {:else}
    <div class="todo-content">
      <p>{todo.title}</p>
      {#if notePreview}
        <button
          class="todo-note"
          type="button"
          title={$translator("todo.editNote")}
          onclick={() => void openNoteEditor()}
        >
          {notePreview}
        </button>
      {/if}
      {#if currentGroup || dueLabel || todo.pinned || todo.priority === 1 || todo.reminder_at !== null || todo.repeat_rule !== null}
        <div class="todo-meta">
          {#if currentGroup}
            <span
              class="todo-group-badge"
              data-group-color={currentGroupColor}
              title={`${$translator("todo.group")}: ${currentGroup.name}`}
            >
              <span class="group-dot" aria-hidden="true"></span>
              {currentGroup.name}
            </span>
          {/if}
          {#if todo.pinned}
            <button
              class="pin-badge"
              type="button"
              title={$translator("todo.unpin")}
              onclick={() => void onPin(todo, false)}
            >
              {$translator("todo.pin")}
            </button>
          {/if}
          {#if todo.priority === 1}
            <button
              class="priority-badge"
              type="button"
              title={$translator("todo.unimportant")}
              onclick={() => void onPriority(todo, 0)}
            >
              {$translator("todo.important")}
            </button>
          {/if}
          {#if dueLabel}
            <button
              class:overdue={dueTone === "overdue"}
              class:today={dueTone === "today"}
              class="due-badge"
              type="button"
              title={$translator("todo.setDue")}
              onclick={toggleSchedule}
            >
              {dueTone === "overdue" ? `${$translator("todo.overdue")} ` : ""}{dueLabel}
            </button>
          {/if}
          {#if todo.reminder_at !== null}
            <button
              class="reminder-badge"
              type="button"
              title={$translator("todo.reminder")}
              onclick={toggleSchedule}
            >
              {$translator("todo.reminderAt", { time: formatReminderLabel(todo.reminder_at) })}
            </button>
          {/if}
          {#if todo.repeat_rule !== null}
            <button
              class="repeat-badge"
              type="button"
              title={$translator("todo.repeat")}
              onclick={toggleSchedule}
            >
              {$translator("todo.repeat")} {repeatLabel(todo.repeat_rule)}
            </button>
          {/if}
        </div>
      {/if}
      {#if scheduleOpen}
        <div class="schedule-popover" role="dialog" aria-label={$translator("todo.setDue")}>
          <strong>{$translator("todo.due")}</strong>
          <div class="schedule-actions">
            <button type="button" disabled={scheduleSaving} onclick={() => void setSchedule(localDateString(0))}>{$translator("todo.today")}</button>
            <button type="button" disabled={scheduleSaving} onclick={() => void setSchedule(localDateString(1))}>{$translator("todo.tomorrow")}</button>
            <button type="button" disabled={scheduleSaving} onclick={() => void setSchedule(localDateString(7))}>{$translator("todo.nextWeek")}</button>
          </div>
          <label>
            <span>{$translator("todo.date")}</span>
            <input
              type="date"
              bind:value={customDate}
              disabled={scheduleSaving}
              onchange={handleCustomDateChange}
            />
          </label>
          <label>
            <span>{$translator("todo.time")}</span>
            <input
              type="time"
              bind:value={customDueTime}
              disabled={scheduleSaving}
            />
          </label>
          <div class="schedule-time-actions" aria-label={$translator("todo.commonDueTimes")}>
            {#each ["09:00", "12:00", "18:00", "21:00"] as time}
              <button
                class:active={customDueTime === time}
                type="button"
                disabled={scheduleSaving}
                onclick={() => (customDueTime = time)}
              >
                {time}
              </button>
            {/each}
          </div>
          <label>
            <span>{$translator("todo.reminder")}</span>
            <select
              bind:value={reminderChoice}
              disabled={scheduleSaving}
              onchange={handleReminderChoiceChange}
            >
              <option value="none">{$translator("todo.noReminder")}</option>
              <option value="same-day-9">{$translator("todo.reminderSameDay")}</option>
              <option value="previous-day-9">{$translator("todo.reminderPreviousDay")}</option>
              <option value="custom">{$translator("todo.reminderCustom")}</option>
            </select>
          </label>
          {#if reminderChoice === "custom"}
            <label>
              <span>{$translator("todo.reminder")}</span>
              <input
                type="datetime-local"
                bind:value={customReminderDateTime}
                disabled={scheduleSaving}
              />
            </label>
          {/if}
          <label>
            <span>{$translator("todo.repeat")}</span>
            <select bind:value={repeatChoice} disabled={scheduleSaving}>
              <option value="none">{$translator("todo.noRepeat")}</option>
              <option value="daily">{$translator("todo.repeatDaily")}</option>
              <option value="weekly">{$translator("todo.repeatWeekly")}</option>
              <option value="monthly">{$translator("todo.repeatMonthly")}</option>
              <option value="weekdays">{$translator("todo.repeatWeekdays")}</option>
            </select>
          </label>
          <div class="schedule-footer">
            <button type="button" disabled={scheduleSaving} onclick={() => void setSchedule(null)}>{$translator("common.clear")}</button>
            <button type="button" disabled={scheduleSaving || !canSaveSchedule} onclick={() => void setSchedule(customDate)}>{$translator("common.save")}</button>
          </div>
          {#if scheduleError}<small>{scheduleError}</small>{/if}
        </div>
      {/if}
      {#if noteOpen}
        <div class="note-popover" role="dialog" aria-label={$translator("todo.editNote")}>
          <strong>{$translator("todo.note")}</strong>
          <textarea
            bind:this={noteInput}
            bind:value={noteDraft}
            maxlength="1000"
            rows="4"
            placeholder={$translator("todo.notePlaceholder")}
            disabled={noteSaving}
            onkeydown={handleNoteKeydown}
          ></textarea>
          <div class="note-footer">
            <small>{noteDraft.length}/1000</small>
            <div>
              <button
                type="button"
                disabled={noteSaving}
                onclick={() => {
                  noteOpen = false;
                  noteError = "";
                }}>{$translator("common.cancel")}</button
              >
              <button
                type="button"
                disabled={noteSaving}
                onclick={() => void saveNote()}>{$translator("common.save")}</button
              >
            </div>
          </div>
          {#if noteError}<small class="note-error">{noteError}</small>{/if}
        </div>
      {/if}
    </div>
  {/if}

  <div class="item-actions">
    <button
      class:active={actionsOpen}
      class="more-button"
      type="button"
      aria-label={`${$translator("common.more")}: ${todo.title}`}
      title={$translator("common.more")}
      aria-haspopup="menu"
      aria-expanded={actionsOpen}
      onclick={toggleActions}
      disabled={editing}
    >
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <circle cx="5" cy="10" r="1.2" />
        <circle cx="10" cy="10" r="1.2" />
        <circle cx="15" cy="10" r="1.2" />
      </svg>
    </button>
    {#if actionsOpen}
      <div class="actions-menu" role="menu">
        <button
          type="button"
          role="menuitem"
          onclick={() => void openNoteEditor()}
        >
          {todo.note ? $translator("todo.editNote") : $translator("todo.addNote")}
        </button>
        <button type="button" role="menuitem" onclick={toggleSchedule}>
          {$translator("todo.setDue")}
        </button>
        {#if groups.length > 0}
          <label class="group-move">
            <span>{$translator("todo.moveTo")}</span>
            <select
              value={todo.group_uuid ?? "ungrouped"}
              disabled={groupSaving}
              onchange={(event) => {
                const value = event.currentTarget.value;
                void moveToGroup(value);
              }}
            >
              <option value="ungrouped">{$translator("todo.noGroup")}</option>
              {#each groups as group (group.uuid)}
                <option value={group.uuid}>{group.name}</option>
              {/each}
            </select>
          </label>
        {/if}
        {#if todo.reminder_at !== null && !todo.completed}
          <button
            type="button"
            role="menuitem"
            onclick={() => {
              actionsOpen = false;
              void onSnooze(todo, snoozeReminderAt());
            }}
          >
            {$translator("todo.snoozeTenMinutes")}
          </button>
          <button
            type="button"
            role="menuitem"
            onclick={() => {
              actionsOpen = false;
              void onSnooze(todo, laterTodayReminderAt());
            }}
          >
            {$translator("todo.laterToday")}
          </button>
        {/if}
        <button
          type="button"
          role="menuitem"
          onclick={() => {
            actionsOpen = false;
            void onPin(todo, !todo.pinned);
          }}
        >
          {todo.pinned ? $translator("todo.unpin") : $translator("todo.pin")}
        </button>
        <button
          type="button"
          role="menuitem"
          onclick={() => {
            actionsOpen = false;
            void onPriority(todo, todo.priority === 1 ? 0 : 1);
          }}
        >
          {todo.priority === 1 ? $translator("todo.unimportant") : $translator("todo.setImportant")}
        </button>
        <button
          type="button"
          role="menuitem"
          onclick={() => {
            actionsOpen = false;
            onFocus(todo);
          }}
        >
          {$translator("todo.startFocus")}
        </button>
        <button
          type="button"
          role="menuitem"
          disabled={reorderDisabled || !canMoveUp}
          onclick={() => {
            actionsOpen = false;
            void onMove(todo, -1);
          }}
        >
          {$translator("todo.moveUp")}
        </button>
        <button
          type="button"
          role="menuitem"
          disabled={reorderDisabled || !canMoveDown}
          onclick={() => {
            actionsOpen = false;
            void onMove(todo, 1);
          }}
        >
          {$translator("todo.moveDown")}
        </button>
        {#if todo.repeat_rule !== null}
          <button
            class="danger"
            type="button"
            role="menuitem"
            onclick={() => {
              actionsOpen = false;
              void onDelete(todo.id, "single");
            }}
          >
            {$translator("todo.deleteOccurrence")}
          </button>
          <button
            class="danger"
            type="button"
            role="menuitem"
            onclick={() => {
              actionsOpen = false;
              void onDelete(todo.id, "series");
            }}
          >
            {$translator("todo.deleteSeries")}
          </button>
        {:else}
          <button
            class="danger"
            type="button"
            role="menuitem"
            onclick={() => {
              actionsOpen = false;
              void onDelete(todo.id);
            }}
          >
            {$translator("common.delete")}
          </button>
        {/if}
      </div>
    {/if}
  </div>
</article>
