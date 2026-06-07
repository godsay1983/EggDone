<script lang="ts">
  import { isTauri } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { flip } from "svelte/animate";
  import { onMount } from "svelte";

  import { todoApi } from "$lib/api/todoApi";
  import {
    initializeDesktopSettings,
    type DesktopSettings,
  } from "$lib/api/desktopSettings";
  import {
    completedCount,
    remainingCount,
    todos,
  } from "$lib/stores/todoStore";
  import type { Todo } from "$lib/types";
  import { movePreviewByPointer } from "$lib/utils/reorderPreview";
  import DataManager from "./DataManager.svelte";
  import SettingsPanel from "./SettingsPanel.svelte";
  import TodoItem from "./TodoItem.svelte";

  type Theme = "light" | "dark";

  let title = "";
  let adding = false;
  let showAbout = false;
  let showDataManager = false;
  let showSettings = false;
  let desktopSettings: DesktopSettings | null = null;
  let theme: Theme = "light";
  let reorderAnimationDuration = 170;
  let inputElement: HTMLInputElement;
  let draggedTodo: Todo | null = null;
  let dragPointerId: number | null = null;
  let previewOrderIds: number[] | null = null;
  let undoTodo: Todo | null = null;
  let undoTimer: ReturnType<typeof setTimeout> | null = null;
  let confirmingClear = false;
  let clearTimer: ReturnType<typeof setTimeout> | null = null;
  $: renderedTodos = applyPreviewOrder($todos.items, previewOrderIds);

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
    reorderAnimationDuration = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches
      ? 0
      : 170;

    void todos.load();
    if (isTauri()) {
      void initializeDesktopSettings()
        .then((settings) => (desktopSettings = settings))
        .catch((error) => todos.reportError(error));
      void listen("focus-new-todo", () => {
        showAbout = false;
        showDataManager = false;
        showSettings = false;
        requestAnimationFrame(() => inputElement?.focus());
      }).then((unlisten) => unlisteners.push(unlisten));
      void listen("show-about", () => {
        showDataManager = false;
        showSettings = false;
        showAbout = true;
      }).then((unlisten) => unlisteners.push(unlisten));
      void listen("single-instance", () => {
        showAbout = false;
        showDataManager = false;
        showSettings = false;
        requestAnimationFrame(() => inputElement?.focus());
      }).then((unlisten) => unlisteners.push(unlisten));
    }

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
      if (undoTimer) clearTimeout(undoTimer);
      if (clearTimer) clearTimeout(clearTimer);
      removeDragListeners();
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

  function startDrag(todo: Todo, event: PointerEvent) {
    cancelDrag();
    draggedTodo = todo;
    dragPointerId = event.pointerId;
    previewOrderIds = $todos.items
      .filter((item) => item.completed === todo.completed)
      .map((item) => item.id);
    window.addEventListener("pointermove", moveDrag, true);
    window.addEventListener("pointerup", endDrag, true);
    window.addEventListener("pointercancel", cancelDrag, true);
  }

  function moveDrag(event: PointerEvent) {
    if (!draggedTodo || event.pointerId !== dragPointerId) return;
    event.preventDefault();
    updateDragTarget(event.clientY);
  }

  function updateDragTarget(clientY: number) {
    if (!draggedTodo) return;
    const source = draggedTodo;
    if (!previewOrderIds) return;

    const rowCenters = Array.from(
      document.querySelectorAll<HTMLElement>("[data-todo-id]"),
    )
      .map((element) => {
        const todo = renderedTodos.find(
          (item) => item.id === Number(element.dataset.todoId),
        );
        const rect = element.getBoundingClientRect();
        return todo && todo.completed === source.completed
          ? { id: todo.id, centerY: rect.top + rect.height / 2 }
          : null;
      })
      .filter(
        (row): row is { id: number; centerY: number } => row !== null,
      );

    const nextOrder = movePreviewByPointer(
      previewOrderIds,
      source.id,
      clientY,
      rowCenters,
    );
    if (nextOrder === previewOrderIds) return;

    previewOrderIds = nextOrder;
  }

  async function endDrag(event: PointerEvent) {
    if (!draggedTodo || event.pointerId !== dragPointerId) return;
    event.preventDefault();
    updateDragTarget(event.clientY);
    const orderedIds = previewOrderIds;
    const sourceCompleted = draggedTodo.completed;
    removeDragListeners();
    if (!orderedIds) {
      resetDragState();
      return;
    }

    const currentIds = $todos.items
      .filter((todo) => todo.completed === sourceCompleted)
      .map((todo) => todo.id);
    if (orderedIds.every((id, index) => id === currentIds[index])) {
      resetDragState();
      return;
    }

    const persistence = todos.reorder(orderedIds);
    resetDragState();
    try {
      await persistence;
    } catch {
      // The store restores the previous order and exposes the error.
    }
  }

  async function moveTodo(todo: Todo, direction: -1 | 1) {
    const group = $todos.items.filter(
      (item) => item.completed === todo.completed,
    );
    const currentIndex = group.findIndex((item) => item.id === todo.id);
    const nextIndex = currentIndex + direction;
    if (currentIndex < 0 || nextIndex < 0 || nextIndex >= group.length) return;

    const reordered = [...group];
    [reordered[currentIndex], reordered[nextIndex]] = [
      reordered[nextIndex],
      reordered[currentIndex],
    ];
    try {
      await todos.reorder(reordered.map((item) => item.id));
    } catch {
      // The store restores the previous order and exposes the error.
    }
  }

  function cancelDrag() {
    removeDragListeners();
    resetDragState();
  }

  function removeDragListeners() {
    window.removeEventListener("pointermove", moveDrag, true);
    window.removeEventListener("pointerup", endDrag, true);
    window.removeEventListener("pointercancel", cancelDrag, true);
  }

  function resetDragState() {
    draggedTodo = null;
    dragPointerId = null;
    previewOrderIds = null;
  }

  function applyPreviewOrder(items: Todo[], orderedIds: number[] | null) {
    if (!orderedIds) return items;

    const order = new Map(orderedIds.map((id, index) => [id, index]));
    const reorderedGroup = items
      .filter((item) => order.has(item.id))
      .sort((left, right) => order.get(left.id)! - order.get(right.id)!);
    let groupIndex = 0;
    return items.map((item) =>
      order.has(item.id) ? reorderedGroup[groupIndex++] : item,
    );
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
        class="settings-button"
        type="button"
        aria-label="设置"
        title="设置"
        onclick={() => {
          showAbout = false;
          showDataManager = false;
          showSettings = true;
        }}
        disabled={!desktopSettings}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.83 2.83-.06-.06a1.7 1.7 0 0 0-1.88-.34 1.7 1.7 0 0 0-1.03 1.56V21h-4v-.08A1.7 1.7 0 0 0 8.96 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06-2.83-2.83.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-1.56-1.03H3v-4h.08A1.7 1.7 0 0 0 4.6 8.96a1.7 1.7 0 0 0-.34-1.88l-.06-.06L7.03 4.2l.06.06a1.7 1.7 0 0 0 1.88.34A1.7 1.7 0 0 0 10 3.04V3h4v.08a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.88-.34l.06-.06 2.83 2.83-.06.06A1.7 1.7 0 0 0 19.4 9c.24.6.83 1 1.48 1H21v4h-.08c-.65 0-1.24.4-1.52 1Z" />
        </svg>
      </button>

      <button
        class="data-button"
        type="button"
        aria-label="数据管理"
        title="数据管理"
        onclick={() => {
          showAbout = false;
          showSettings = false;
          showDataManager = true;
        }}
      >
        <svg viewBox="0 0 20 20" aria-hidden="true">
          <ellipse cx="10" cy="5" rx="5.5" ry="2.5" />
          <path d="M4.5 5v5c0 1.4 2.5 2.5 5.5 2.5s5.5-1.1 5.5-2.5V5M4.5 10v5c0 1.4 2.5 2.5 5.5 2.5s5.5-1.1 5.5-2.5v-5" />
        </svg>
      </button>

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
      {#each renderedTodos as todo (todo.id)}
        {@const group = renderedTodos.filter((item) => item.completed === todo.completed)}
        {@const groupIndex = group.findIndex((item) => item.id === todo.id)}
        <div
          class="todo-row"
          animate:flip={{ duration: reorderAnimationDuration }}
        >
          <TodoItem
            {todo}
            onToggle={toggleTodo}
            onEdit={editTodo}
            onDelete={deleteTodo}
            onMove={moveTodo}
            onDragStart={startDrag}
            canMoveUp={groupIndex > 0}
            canMoveDown={groupIndex < group.length - 1}
            isDragging={draggedTodo?.id === todo.id}
            isDragTarget={draggedTodo?.id === todo.id}
          />
        </div>
      {/each}
    {/if}
  </section>

  <footer>
    <span>一步一点，不着急</span>
    <button
      type="button"
      onclick={() => {
        showDataManager = false;
        showSettings = false;
        showAbout = true;
      }}>关于</button
    >
  </footer>
</main>

{#if showDataManager}
  <DataManager
    onClose={() => (showDataManager = false)}
    onImported={async () => todos.load()}
  />
{/if}

{#if showSettings && desktopSettings}
  <SettingsPanel
    settings={desktopSettings}
    onClose={() => (showSettings = false)}
    onChange={(settings) => (desktopSettings = settings)}
  />
{/if}

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
