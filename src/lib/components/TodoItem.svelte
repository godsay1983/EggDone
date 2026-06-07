<script lang="ts">
  import { onMount, tick } from "svelte";
  import { fly } from "svelte/transition";

  import type { Todo } from "$lib/types";

  export let todo: Todo;
  export let onToggle: (todo: Todo) => Promise<void>;
  export let onEdit: (id: number, title: string) => Promise<void>;
  export let onDelete: (id: number) => Promise<void>;
  export let onMove: (todo: Todo, direction: -1 | 1) => Promise<void>;
  export let onDragStart: (todo: Todo, event: PointerEvent) => void;
  export let canMoveUp = false;
  export let canMoveDown = false;
  export let isDragging = false;
  export let isDragTarget = false;

  let editing = false;
  let editTitle = "";
  let editError = "";
  let saving = false;
  let editInput: HTMLInputElement;
  let itemElement: HTMLElement;
  let animationDuration = 140;

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
    }

    window.addEventListener("pointerdown", handlePointerDown, true);
    return () => window.removeEventListener("pointerdown", handlePointerDown, true);
  });

  async function beginEdit() {
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
    title="拖动排序"
    onpointerdown={(event) => {
      if (event.button !== 0) return;
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
    <p>{todo.title}</p>
  {/if}

  <div class="item-actions">
    <button
      class="move-button"
      type="button"
      aria-label={`上移任务：${todo.title}`}
      title="上移"
      onclick={() => void onMove(todo, -1)}
      disabled={editing || !canMoveUp}
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
      disabled={editing || !canMoveDown}
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
