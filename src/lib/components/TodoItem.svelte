<script lang="ts">
  import { onMount, tick } from "svelte";
  import { fly } from "svelte/transition";

  import type { TodoScheduleInput } from "$lib/api/todoApi";
  import type { Todo } from "$lib/types";

  export let todo: Todo;
  export let onToggle: (todo: Todo) => Promise<void>;
  export let onEdit: (id: number, title: string) => Promise<void>;
  export let onPin: (todo: Todo, pinned: boolean) => Promise<void>;
  export let onSchedule: (id: number, schedule: TodoScheduleInput) => Promise<void>;
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
  let editInput: HTMLInputElement;
  let itemElement: HTMLElement;
  let animationDuration = 140;
  $: dueLabel = formatDueLabel(todo);
  $: dueTone = getDueTone(todo);

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
    }

    window.addEventListener("pointerdown", handlePointerDown, true);
    return () => window.removeEventListener("pointerdown", handlePointerDown, true);
  });

  async function beginEdit() {
    scheduleOpen = false;
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
    scheduleOpen = !scheduleOpen;
    scheduleError = "";
    customDate = todo.due_date ?? localDateString(0);
  }

  async function setDateOnly(date: string | null) {
    if (scheduleSaving) return;
    scheduleSaving = true;
    scheduleError = "";
    try {
      await onSchedule(todo.id, {
        due_date: date,
        due_at: null,
        reminder_at: null,
      });
      scheduleOpen = false;
    } catch {
      scheduleError = "日期保存失败，请重试";
    } finally {
      scheduleSaving = false;
    }
  }

  function localDateString(offsetDays: number) {
    const date = new Date();
    date.setDate(date.getDate() + offsetDays);
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    return `${year}-${month}-${day}`;
  }

  function formatDueLabel(item: Todo) {
    if (item.due_date) {
      const today = localDateString(0);
      const tomorrow = localDateString(1);
      if (item.due_date === today) return "今天";
      if (item.due_date === tomorrow) return "明天";
      return item.due_date.slice(5).replace("-", "/");
    }
    if (item.due_at !== null) {
      return new Intl.DateTimeFormat("zh-CN", {
        month: "numeric",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      }).format(new Date(item.due_at));
    }
    return "";
  }

  function getDueTone(item: Todo) {
    if (item.completed) return "";
    const today = localDateString(0);
    if (item.due_date && item.due_date < today) return "overdue";
    if (item.due_date === today) return "today";
    if (item.due_at !== null && item.due_at < Date.now()) return "overdue";
    return "";
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
            <input type="date" bind:value={customDate} disabled={scheduleSaving} />
          </label>
          <div class="schedule-footer">
            <button type="button" disabled={scheduleSaving} onclick={() => void setDateOnly(null)}>清除</button>
            <button type="button" disabled={scheduleSaving || !customDate} onclick={() => void setDateOnly(customDate)}>保存</button>
          </div>
          {#if scheduleError}<small>{scheduleError}</small>{/if}
        </div>
      {/if}
    </div>
  {/if}

  <div class="item-actions">
    <button
      class:active={Boolean(todo.due_date || todo.due_at)}
      class="schedule-button"
      type="button"
      aria-label={`设置到期日期：${todo.title}`}
      title="设置到期日期"
      onclick={toggleSchedule}
      disabled={editing}
    >
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <rect x="4" y="5" width="12" height="11" rx="2" />
        <path d="M7 3v4M13 3v4M4 9h12" />
      </svg>
    </button>
    <button
      class:active={todo.pinned}
      class="pin-button"
      type="button"
      aria-label={todo.pinned ? `取消置顶：${todo.title}` : `置顶任务：${todo.title}`}
      title={todo.pinned ? "取消置顶" : "置顶"}
      onclick={() => void onPin(todo, !todo.pinned)}
      disabled={editing}
    >
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <path d="M7 3h6l-1 5 3 3H5l3-3-1-5Zm3 8v6" />
      </svg>
    </button>
    <button
      class="move-button"
      type="button"
      aria-label={`上移任务：${todo.title}`}
      title="上移"
      onclick={() => void onMove(todo, -1)}
      disabled={editing || reorderDisabled || !canMoveUp}
    >
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <path d="m6 12 4-4 4 4" />
      </svg>
    </button>
    <button
      class="move-button"
      type="button"
      aria-label={`下移任务：${todo.title}`}
      title="下移"
      onclick={() => void onMove(todo, 1)}
      disabled={editing || reorderDisabled || !canMoveDown}
    >
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <path d="m6 8 4 4 4-4" />
      </svg>
    </button>
    <button
      class="delete-button"
      type="button"
      aria-label={`删除任务：${todo.title}`}
      title="删除任务"
      onclick={() => void onDelete(todo.id)}
      disabled={editing}
    >
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <path d="M5 6h10M8 6V4h4v2m2 0-.6 10H6.6L6 6m3 3v4m2-4v4" />
      </svg>
    </button>
  </div>
</article>
