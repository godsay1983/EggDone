<script lang="ts">
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import { todoApi } from "$lib/api/todoApi";
  import { remainingCount, todos } from "$lib/stores/todoStore";
  import type { Todo } from "$lib/types";
  import TodoItem from "./TodoItem.svelte";

  let title = "";
  let adding = false;
  let showAbout = false;
  let inputElement: HTMLInputElement;

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];

    void todos.load();
    void listen("focus-new-todo", () => {
      showAbout = false;
      requestAnimationFrame(() => inputElement?.focus());
    }).then((unlisten) => unlisteners.push(unlisten));
    void listen("show-about", () => {
      showAbout = true;
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => unlisteners.forEach((unlisten) => unlisten());
  });

  async function addTodo() {
    const nextTitle = title.trim();
    if (!nextTitle || adding) return;

    adding = true;
    try {
      await todos.add(nextTitle);
      title = "";
    } catch (error) {
      todos.reportError(error);
    } finally {
      adding = false;
      inputElement?.focus();
    }
  }

  async function toggleTodo(todo: Todo) {
    try {
      await todos.toggle(todo);
    } catch (error) {
      todos.reportError(error);
    }
  }

  async function deleteTodo(id: number) {
    try {
      await todos.remove(id);
    } catch (error) {
      todos.reportError(error);
    }
  }
</script>

<main class="panel-shell">
  <header class="panel-header">
    <div class="brand">
      <div class="mascot" aria-hidden="true">
        <span class="eye left"></span>
        <span class="eye right"></span>
        <span class="mouth"></span>
        <span class="check">✓</span>
      </div>
      <div>
        <h1>蛋定 Todo</h1>
        <p>拖拖蛋陪你慢慢完成</p>
      </div>
    </div>

    <button class="close-button" type="button" aria-label="隐藏面板" title="隐藏面板" onclick={() => todoApi.hidePanel()}>
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <path d="m5 5 10 10m0-10L5 15" />
      </svg>
    </button>
  </header>

  <form class="quick-add" onsubmit={(event) => { event.preventDefault(); void addTodo(); }}>
    <input
      bind:this={inputElement}
      bind:value={title}
      maxlength="200"
      placeholder="准备完成什么？按回车添加"
      aria-label="新任务内容"
      autocomplete="off"
    />
    <button type="submit" disabled={!title.trim() || adding} aria-label="添加任务">
      {adding ? "…" : "+"}
    </button>
  </form>

  <section class="summary">
    <span>待办清单</span>
    <span class="count">{$remainingCount} 项未完成</span>
  </section>

  <section class="todo-list" aria-live="polite">
    {#if $todos.loading}
      <div class="status">正在唤醒拖拖蛋…</div>
    {:else if $todos.error}
      <div class="status error">
        <span>{$todos.error}</span>
        <button type="button" onclick={() => todos.load()}>重试</button>
      </div>
    {:else if $todos.items.length === 0}
      <div class="empty-state">
        <div class="empty-yolk" aria-hidden="true">✓</div>
        <strong>今天也要蛋定完成</strong>
        <span>先写下一件小事吧</span>
      </div>
    {:else}
      {#each $todos.items as todo (todo.id)}
        <TodoItem {todo} onToggle={toggleTodo} onDelete={deleteTodo} />
      {/each}
    {/if}
  </section>

  <footer>
    <span>一步一点，不着急</span>
    <button type="button" onclick={() => showAbout = true}>关于</button>
  </footer>
</main>

{#if showAbout}
  <div class="about-backdrop">
    <button class="about-dismiss" type="button" aria-label="关闭关于窗口" onclick={() => showAbout = false}></button>
    <div class="about-card" role="dialog" aria-modal="true" aria-labelledby="about-title">
      <div class="about-mascot" aria-hidden="true">✓</div>
      <h2 id="about-title">EggDone</h2>
      <p>蛋定 Todo 0.1.0</p>
      <small>原创角色「拖拖蛋」陪你轻松处理待办。</small>
      <button type="button" onclick={() => showAbout = false}>知道了</button>
    </div>
  </div>
{/if}
