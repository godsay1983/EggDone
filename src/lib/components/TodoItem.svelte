<script lang="ts">
  import type { Todo } from "$lib/types";

  export let todo: Todo;
  export let onToggle: (todo: Todo) => void;
  export let onDelete: (id: number) => void;
</script>

<article class:completed={todo.completed} class="todo-item">
  <button
    class="checkbox"
    class:checked={todo.completed}
    type="button"
    aria-label={todo.completed ? "标记为未完成" : "标记为已完成"}
    onclick={() => onToggle(todo)}
  >
    {#if todo.completed}
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <path d="m4 10 4 4 8-9" />
      </svg>
    {/if}
  </button>

  <p>{todo.title}</p>

  <button
    class="delete-button"
    type="button"
    aria-label={`删除任务：${todo.title}`}
    title="删除任务"
    onclick={() => onDelete(todo.id)}
  >
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path d="M5 6h10M8 6V4h4v2m2 0-.6 10H6.6L6 6m3 3v4m2-4v4" />
    </svg>
  </button>
</article>
