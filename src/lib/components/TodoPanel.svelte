<script lang="ts">
  import { isTauri } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import { todoApi } from "$lib/api/todoApi";
  import {
    completedCount,
    remainingCount,
    todos,
  } from "$lib/stores/todoStore";
  import type { Todo } from "$lib/types";
  import TodoItem from "./TodoItem.svelte";

  type Theme = "light" | "dark";

  let title = "";
  let adding = false;
  let showAbout = false;
  let theme: Theme = "light";
  let inputElement: HTMLInputElement;
  let draggedTodo: Todo | null = null;
  let undoTodo: Todo | null = null;
  let undoTimer: ReturnType<typeof setTimeout> | null = null;
  let confirmingClear = false;
  let clearTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];
    const savedTheme = localStorage.getItem("eggdone-theme");
    theme =
      savedTheme === "light" || savedTheme === "dark"
        ? savedTheme
        : window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light";
    applyTheme(theme);

    void todos.load();
    if (isTauri()) {
      void listen("focus-new-todo", () => {
        showAbout = false;
        requestAnimationFrame(() => inputElement?.focus());
      }).then((unlisten) => unlisteners.push(unlisten));
      void listen("show-about", () => {
        showAbout = true;
      }).then((unlisten) => unlisteners.push(unlisten));
      void listen("single-instance", () => {
        showAbout = false;
        requestAnimationFrame(() => inputElement?.focus());
      }).then((unlisten) => unlisteners.push(unlisten));
    }

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
      if (undoTimer) clearTimeout(undoTimer);
      if (clearTimer) clearTimeout(clearTimer);
    };
  });

  function toggleTheme() {
    theme = theme === "light" ? "dark" : "light";
    localStorage.setItem("eggdone-theme", theme);
    applyTheme(theme);
  }

  function applyTheme(nextTheme: Theme) {
    document.documentElement.dataset.theme = nextTheme;
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", nextTheme === "dark" ? "#1d1b18" : "#f6c94c");
  }

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

  async function editTodo(id: number, nextTitle: string) {
    try {
      await todos.edit(id, nextTitle);
    } catch (error) {
      todos.reportError(error);
      throw error;
    }
  }

  async function deleteTodo(id: number) {
    try {
      undoTodo = await todos.remove(id);
      if (undoTimer) clearTimeout(undoTimer);
      undoTimer = setTimeout(() => {
        undoTodo = null;
        undoTimer = null;
      }, 5000);
    } catch (error) {
      todos.reportError(error);
    }
  }

  async function undoDelete() {
    if (!undoTodo) return;
    const todoToRestore = undoTodo;
    undoTodo = null;
    if (undoTimer) {
      clearTimeout(undoTimer);
      undoTimer = null;
    }
    try {
      await todos.restore(todoToRestore.id);
    } catch (error) {
      undoTodo = todoToRestore;
      todos.reportError(error);
    }
  }

  function requestClearCompleted() {
    if (confirmingClear) {
      void clearCompleted();
      return;
    }
    confirmingClear = true;
    if (clearTimer) clearTimeout(clearTimer);
    clearTimer = setTimeout(() => {
      confirmingClear = false;
      clearTimer = null;
    }, 3000);
  }

  async function clearCompleted() {
    confirmingClear = false;
    if (clearTimer) {
      clearTimeout(clearTimer);
      clearTimer = null;
    }
    try {
      await todos.clearCompleted();
    } catch (error) {
      todos.reportError(error);
    }
  }

  function startDrag(todo: Todo) {
    draggedTodo = todo;
  }

  async function dropTodo(target: Todo) {
    const source = draggedTodo;
    draggedTodo = null;
    if (!source || source.id === target.id || source.completed !== target.completed) {
      return;
    }

    const group = $todos.items.filter(
      (todo) => todo.completed === source.completed,
    );
    const sourceIndex = group.findIndex((todo) => todo.id === source.id);
    const targetIndex = group.findIndex((todo) => todo.id === target.id);
    if (sourceIndex < 0 || targetIndex < 0) return;

    const reordered = [...group];
    const [moved] = reordered.splice(sourceIndex, 1);
    reordered.splice(targetIndex, 0, moved);
    try {
      await todos.reorder(reordered.map((todo) => todo.id));
    } catch {
      // The store restores the previous order and exposes the error.
    }
  }
</script>

<main class="panel-shell">
  <header class="panel-header">
    <div class="brand">
      <img class="mascot" src="/eggdone-icon.png" alt="" aria-hidden="true" />
      <div>
        <h1>蛋定 Todo</h1>
        <p>拖拖蛋陪你慢慢完成</p>
      </div>
    </div>

    <div class="header-actions">
      <button
        class="theme-button"
        type="button"
        aria-label={theme === "light" ? "切换到暗色主题" : "切换到亮色主题"}
        title={theme === "light" ? "切换到暗色主题" : "切换到亮色主题"}
        onclick={toggleTheme}
      >
        {#if theme === "light"}
          <svg viewBox="0 0 20 20" aria-hidden="true">
            <path d="M16.2 12.7A6.4 6.4 0 0 1 7.3 3.8 6.5 6.5 0 1 0 16.2 12.7Z" />
          </svg>
        {:else}
          <svg viewBox="0 0 20 20" aria-hidden="true">
            <circle cx="10" cy="10" r="3.2" />
            <path d="M10 2v1.5M10 16.5V18M2 10h1.5M16.5 10H18M4.3 4.3l1 1M14.7 14.7l1 1M15.7 4.3l-1 1M5.3 14.7l-1 1" />
          </svg>
        {/if}
      </button>

      <button class="close-button" type="button" aria-label="隐藏面板" title="隐藏面板" onclick={() => todoApi.hidePanel()}>
        <svg viewBox="0 0 20 20" aria-hidden="true">
          <path d="m5 5 10 10m0-10L5 15" />
        </svg>
      </button>
    </div>
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
    <div class="summary-actions">
      {#if $completedCount > 0}
        <button
          class:confirming={confirmingClear}
          type="button"
          onclick={requestClearCompleted}
        >
          {confirmingClear ? "确认清除？" : "清除已完成"}
        </button>
      {/if}
      <span class="count">{$remainingCount} 项未完成</span>
    </div>
  </section>

  <section class="todo-list" aria-live="polite">
    {#if $todos.loading}
      <div class="status">正在唤醒拖拖蛋…</div>
    {:else if $todos.error && $todos.items.length === 0}
      <div class="status error">
        <span>{$todos.error}</span>
        <button type="button" onclick={() => todos.load()}>重试</button>
      </div>
    {:else if $todos.items.length === 0}
      <div class="empty-state">
        <img class="empty-mascot" src="/eggdone-icon.png" alt="" aria-hidden="true" />
        <strong>今天也要蛋定完成</strong>
        <span>先写下一件小事吧</span>
      </div>
    {:else}
      {#if $todos.error}
        <div class="inline-error" role="alert">{$todos.error}</div>
      {/if}
      {#each $todos.items as todo (todo.id)}
        <TodoItem
          {todo}
          onToggle={toggleTodo}
          onEdit={editTodo}
          onDelete={deleteTodo}
          onDragStart={startDrag}
          onDrop={dropTodo}
        />
      {/each}
    {/if}
  </section>

  <footer>
    <span>一步一点，不着急</span>
    <button type="button" onclick={() => showAbout = true}>关于</button>
  </footer>
</main>

{#if undoTodo}
  <div class="undo-toast" role="status">
    <span>已删除“{undoTodo.title}”</span>
    <button type="button" onclick={() => void undoDelete()}>撤销</button>
  </div>
{/if}

{#if showAbout}
  <div class="about-backdrop">
    <button class="about-dismiss" type="button" aria-label="关闭关于窗口" onclick={() => showAbout = false}></button>
    <div class="about-card" role="dialog" aria-modal="true" aria-labelledby="about-title">
      <img class="about-mascot" src="/eggdone-icon.png" alt="" aria-hidden="true" />
      <h2 id="about-title">EggDone</h2>
      <p>蛋定 Todo 0.1.0</p>
      <small>原创角色「拖拖蛋」陪你轻松处理待办。</small>
      <button type="button" onclick={() => showAbout = false}>知道了</button>
    </div>
  </div>
{/if}
