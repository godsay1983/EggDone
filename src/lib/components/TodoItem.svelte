<script lang="ts">
  import { onMount, tick } from "svelte";
  import { fly } from "svelte/transition";

  import type { TodoScheduleInput } from "$lib/api/todoApi";
  import type { Todo, TodoGroup } from "$lib/types";
  import {
    formatDueLabel,
    getDueTone,
    localDateString,
  } from "$lib/utils/todoDates";
  import {
    defaultCustomReminderDateTime,
    inferReminderChoice,
    laterTodayReminderAt,
    reminderAtForDate,
    snoozeReminderAt,
    timestampToDateTimeLocal,
    type ReminderChoice,
  } from "$lib/utils/reminderTimes";

  export let todo: Todo;
  export let onToggle: (todo: Todo) => Promise<void>;
  export let onEdit: (id: number, title: string) => Promise<void>;
  export let onPin: (todo: Todo, pinned: boolean) => Promise<void>;
  export let onSchedule: (id: number, schedule: TodoScheduleInput) => Promise<void>;
  export let onSnooze: (todo: Todo, reminderAt: number) => Promise<void>;
  export let groups: TodoGroup[] = [];
  export let onGroupChange: (todo: Todo, groupUuid: string | null) => Promise<void>;
  export let onDelete: (id: number) => Promise<void>;
  export let onMove: (todo: Todo, direction: -1 | 1) => Promise<void>;
  export let onDragStart: (todo: Todo, event: PointerEvent) => void;
  export let canMoveUp = false;
  export let canMoveDown = false;
  export let isDragging = false;
  export let isDragTarget = false;
  export let reorderDisabled = false;

  let editing = false;
  let editTitle = "";
  let editError = "";
  let saving = false;
  let scheduleOpen = false;
  let scheduleSaving = false;
  let scheduleError = "";
  let customDate = "";
  let customReminderDateTime = "";
  let reminderChoice: ReminderChoice = "none";
  let groupSaving = false;
  let actionsOpen = false;
  let editInput: HTMLInputElement;
  let itemElement: HTMLElement;
  let animationDuration = 140;
  $: dueLabel = formatDueLabel(todo);
  $: dueTone = getDueTone(todo);
  $: canSaveSchedule =
    Boolean(customDate) &&
    (reminderChoice !== "custom" || Boolean(customReminderDateTime));

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
    scheduleOpen = !scheduleOpen;
    scheduleError = "";
    customDate = todo.due_date ?? localDateString(0);
    reminderChoice = inferReminderChoice(todo.due_date, todo.reminder_at);
    customReminderDateTime =
      todo.reminder_at !== null
        ? timestampToDateTimeLocal(todo.reminder_at)
        : defaultCustomReminderDateTime(customDate);
  }

  function toggleActions() {
    if (editing) return;
    scheduleOpen = false;
    actionsOpen = !actionsOpen;
  }

  async function setDateOnly(date: string | null) {
    if (scheduleSaving) return;
    const reminderAt = date
      ? reminderAtForDate(date, reminderChoice, customReminderDateTime)
      : null;
    if (date && reminderChoice === "custom" && reminderAt === null) {
      scheduleError = "请选择提醒时间";
      return;
    }

    scheduleSaving = true;
    scheduleError = "";
    try {
      await onSchedule(todo.id, {
        due_date: date,
        due_at: null,
        reminder_at: reminderAt,
      });
      scheduleOpen = false;
    } catch {
      scheduleError = "日期保存失败，请重试";
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
    return new Intl.DateTimeFormat("zh-CN", {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(reminderAt));
  }

  async function moveToGroup(value: string) {
    if (groupSaving) return;
    groupSaving = true;
    try {
      await onGroupChange(todo, value === "ungrouped" ? null : value);
      actionsOpen = false;
    } finally {
      groupSaving = false;
    }
  }

  async function saveEdit() {
    if (saving) return;

    const nextTitle = editTitle.trim();
    if (!nextTitle) {
      editError = "任务内容不能为空";
      return;
    }
    if (nextTitle === todo.title) {
      cancelEdit();
      return;
    }

    saving = true;
    editError = "";
    try {
      await onEdit(todo.id, nextTitle);
      editing = false;
    } catch {
      editError = "保存失败，请重试";
      await tick();
      editInput?.focus();
    } finally {
      saving = false;
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
  <button
    class="drag-handle"
    type="button"
    aria-label={`拖动排序：${todo.title}`}
    title={reorderDisabled ? "搜索时不可排序" : "拖动排序"}
    disabled={reorderDisabled}
    onpointerdown={(event) => {
      if (event.button !== 0 || reorderDisabled) return;
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
    aria-label={todo.completed ? "标记为未完成" : "标记为已完成"}
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
        aria-label={`编辑任务：${todo.title}`}
        disabled={saving}
        onkeydown={handleEditKeydown}
      />
      {#if editError}<small>{editError}</small>{/if}
    </div>
  {:else}
    <div class="todo-content">
      <p>{todo.title}</p>
      {#if dueLabel || todo.pinned || todo.reminder_at !== null}
        <div class="todo-meta">
          {#if todo.pinned}
            <button
              class="pin-badge"
              type="button"
              title="取消置顶"
              onclick={() => void onPin(todo, false)}
            >
              置顶
            </button>
          {/if}
          {#if dueLabel}
            <button
              class:overdue={dueTone === "overdue"}
              class:today={dueTone === "today"}
              class="due-badge"
              type="button"
              title="修改到期日期"
              onclick={toggleSchedule}
            >
              {dueTone === "overdue" ? "逾期 " : ""}{dueLabel}
            </button>
          {/if}
          {#if todo.reminder_at !== null}
            <button
              class="reminder-badge"
              type="button"
              title="修改提醒时间"
              onclick={toggleSchedule}
            >
              提醒 {formatReminderLabel(todo.reminder_at)}
            </button>
          {/if}
        </div>
      {/if}
      {#if scheduleOpen}
        <div class="schedule-popover" role="dialog" aria-label="设置到期日期">
          <strong>到期日期</strong>
          <div class="schedule-actions">
            <button type="button" disabled={scheduleSaving} onclick={() => void setDateOnly(localDateString(0))}>今天</button>
            <button type="button" disabled={scheduleSaving} onclick={() => void setDateOnly(localDateString(1))}>明天</button>
            <button type="button" disabled={scheduleSaving} onclick={() => void setDateOnly(localDateString(7))}>下周</button>
          </div>
          <label>
            <span>自定义</span>
            <input
              type="date"
              bind:value={customDate}
              disabled={scheduleSaving}
              onchange={handleCustomDateChange}
            />
          </label>
          <label>
            <span>提醒</span>
            <select
              bind:value={reminderChoice}
              disabled={scheduleSaving}
              onchange={handleReminderChoiceChange}
            >
              <option value="none">不提醒</option>
              <option value="same-day-9">当天 9:00</option>
              <option value="previous-day-9">提前一天 9:00</option>
              <option value="custom">指定时间</option>
            </select>
          </label>
          {#if reminderChoice === "custom"}
            <label>
              <span>提醒时间</span>
              <input
                type="datetime-local"
                bind:value={customReminderDateTime}
                disabled={scheduleSaving}
              />
            </label>
          {/if}
          <div class="schedule-footer">
            <button type="button" disabled={scheduleSaving} onclick={() => void setDateOnly(null)}>清除</button>
            <button type="button" disabled={scheduleSaving || !canSaveSchedule} onclick={() => void setDateOnly(customDate)}>保存</button>
          </div>
          {#if scheduleError}<small>{scheduleError}</small>{/if}
        </div>
      {/if}
    </div>
  {/if}

  <div class="item-actions">
    <button
      class:active={actionsOpen}
      class="more-button"
      type="button"
      aria-label={`更多操作：${todo.title}`}
      title="更多操作"
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
        <button type="button" role="menuitem" onclick={toggleSchedule}>
          设置日期
        </button>
        {#if groups.length > 0}
          <label class="group-move">
            <span>移动到</span>
            <select
              value={todo.group_uuid ?? "ungrouped"}
              disabled={groupSaving}
              onchange={(event) => {
                const value = event.currentTarget.value;
                void moveToGroup(value);
              }}
            >
              <option value="ungrouped">未分组</option>
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
            稍后 10 分钟
          </button>
          <button
            type="button"
            role="menuitem"
            onclick={() => {
              actionsOpen = false;
              void onSnooze(todo, laterTodayReminderAt());
            }}
          >
            今天晚些时候
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
          {todo.pinned ? "取消置顶" : "置顶"}
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
          上移
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
          下移
        </button>
        <button
          class="danger"
          type="button"
          role="menuitem"
          onclick={() => {
            actionsOpen = false;
            void onDelete(todo.id);
          }}
        >
          删除
        </button>
      </div>
    {/if}
  </div>
</article>
