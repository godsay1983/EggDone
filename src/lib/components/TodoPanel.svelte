<script lang="ts">
  import { isTauri } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { flip } from "svelte/animate";
  import { onMount, tick } from "svelte";

  import { todoApi } from "$lib/api/todoApi";
  import type { TodoScheduleInput } from "$lib/api/todoApi";
  import {
    initializeDesktopSettings,
    type DesktopSettings,
  } from "$lib/api/desktopSettings";
  import {
    completedCount,
    remainingCount,
    todos,
  } from "$lib/stores/todoStore";
  import { initializeAutoSync, syncStatus } from "$lib/sync/autoSync";
  import type { RepeatDeleteScope, Todo, TodoGroup } from "$lib/types";
  import { movePreviewByPointer } from "$lib/utils/reorderPreview";
  import { isDueTodayOrOverdue } from "$lib/utils/todoDates";
  import {
    filterTodos,
    type TodoListView,
  } from "$lib/utils/todoFilters";
  import { parseQuickAdd } from "$lib/utils/quickAdd";
  import {
    DEFAULT_LIST_VIEW_KEY,
    LAST_LIST_VIEW_KEY,
    initialListView,
    normalizeDefaultListViewMode,
    type DefaultListViewMode,
  } from "$lib/utils/viewPreferences";
  import DataManager from "./DataManager.svelte";
  import SettingsPanel from "./SettingsPanel.svelte";
  import TodoItem from "./TodoItem.svelte";

  type Theme = "light" | "dark";
  type GroupColor = "yellow" | "green" | "blue" | "peach" | "lavender" | "gray";
  type GroupDropTarget = string | null;

  const groupColorOptions: Array<{ value: GroupColor; label: string }> = [
    { value: "yellow", label: "蛋黄" },
    { value: "green", label: "薄荷" },
    { value: "blue", label: "晴空" },
    { value: "peach", label: "蜜桃" },
    { value: "lavender", label: "薰衣草" },
    { value: "gray", label: "米灰" },
  ];

  let title = "";
  let quickAddParsingDisabledFor = "";
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
  let dragGroupTarget: GroupDropTarget | undefined = undefined;
  let undoTodos: Todo[] = [];
  let undoTimer: ReturnType<typeof setTimeout> | null = null;
  let confirmingClear = false;
  let clearTimer: ReturnType<typeof setTimeout> | null = null;
  let showSearch = false;
  let searchQuery = "";
  let showCompleted = true;
  let listView: TodoListView = "all";
  let defaultListViewMode: DefaultListViewMode = "remember";
  let selectedGroup = "all";
  let creatingGroup = false;
  let groupName = "";
  let groupSaving = false;
  let managingGroup = false;
  let editingGroupName = "";
  let groupDeleting = false;
  let confirmingGroupDelete = false;
  let groupDeleteTimer: ReturnType<typeof setTimeout> | null = null;
  let searchInput: HTMLInputElement;
  $: searchActive = searchQuery.trim().length > 0;
  $: reorderDisabled = searchActive || listView === "today";
  $: todayCount = $todos.items.filter((todo) => isDueTodayOrOverdue(todo)).length;
  $: activeGroupUuid = groupFilterValue(selectedGroup);
  $: selectedGroupObject = $todos.groups.find(
    (group) => group.uuid === selectedGroup,
  );
  $: selectedGroupIndex = $todos.groups.findIndex(
    (group) => group.uuid === selectedGroup,
  );
  $: filteredTodos = filterTodos($todos.items, searchQuery, showCompleted, {
    view: listView,
    groupUuid: activeGroupUuid,
  });
  $: renderedTodos = applyPreviewOrder(filteredTodos, previewOrderIds);
  $: quickAddResult = parseQuickAdd(
    title,
    new Date(),
    $todos.groups.map((group) => group.name),
  );
  $: quickAddPreview =
    title.trim().length > 0 &&
    quickAddParsingDisabledFor !== title &&
    (quickAddResult.schedule !== null || quickAddResult.groupName !== null)
      ? quickAddResult
      : null;

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];
    const savedTheme = localStorage.getItem("eggdone-theme");
    showCompleted =
      localStorage.getItem("eggdone-show-completed") !== "false";
    defaultListViewMode = normalizeDefaultListViewMode(
      localStorage.getItem(DEFAULT_LIST_VIEW_KEY),
    );
    listView = initialListView(
      defaultListViewMode,
      localStorage.getItem(LAST_LIST_VIEW_KEY),
    );
    selectedGroup = localStorage.getItem("eggdone-selected-group") ?? "all";
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
      void initializeAutoSync();
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
      void listen("show-today", () => {
        showAbout = false;
        showDataManager = false;
        showSettings = false;
        showSearch = false;
        searchQuery = "";
        setListView("today");
        requestAnimationFrame(() => inputElement?.focus());
      }).then((unlisten) => unlisteners.push(unlisten));
      void listen("single-instance", () => {
        showAbout = false;
        showDataManager = false;
        showSettings = false;
        requestAnimationFrame(() => inputElement?.focus());
      }).then((unlisten) => unlisteners.push(unlisten));
      void listen("todos-changed", () => {
        void todos.refresh();
      }).then((unlisten) => unlisteners.push(unlisten));
    }

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
      if (undoTimer) clearTimeout(undoTimer);
      if (clearTimer) clearTimeout(clearTimer);
      if (groupDeleteTimer) clearTimeout(groupDeleteTimer);
      removeDragListeners();
    };
  });

  function toggleTheme() {
    theme = theme === "light" ? "dark" : "light";
    localStorage.setItem("eggdone-theme", theme);
    applyTheme(theme);
  }

  async function toggleSearch() {
    showSearch = !showSearch;
    if (!showSearch) {
      searchQuery = "";
      cancelDrag();
      inputElement?.focus();
      return;
    }

    await tick();
    searchInput?.focus();
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    if (searchQuery) {
      searchQuery = "";
      return;
    }
    void toggleSearch();
  }

  function toggleCompletedVisibility() {
    showCompleted = !showCompleted;
    localStorage.setItem("eggdone-show-completed", String(showCompleted));
    cancelDrag();
  }

  function setListView(view: TodoListView) {
    listView = view;
    localStorage.setItem(LAST_LIST_VIEW_KEY, view);
    cancelDrag();
  }

  function setDefaultListViewMode(mode: DefaultListViewMode) {
    defaultListViewMode = mode;
    localStorage.setItem(DEFAULT_LIST_VIEW_KEY, mode);
    if (mode !== "remember") {
      setListView(mode);
    }
  }

  function setSelectedGroup(group: string) {
    selectedGroup = group;
    localStorage.setItem("eggdone-selected-group", group);
    managingGroup = false;
    confirmingGroupDelete = false;
    cancelDrag();
  }

  function groupFilterValue(group: string) {
    if (group === "all") return undefined;
    if (group === "ungrouped") return null;
    return group;
  }

  function newTodoGroupUuid() {
    return selectedGroup === "all" || selectedGroup === "ungrouped"
      ? null
      : selectedGroup;
  }

  function groupUuidByName(name: string | null) {
    if (name === null) return null;
    return $todos.groups.find((group) => group.name === name)?.uuid ?? null;
  }

  function groupLabel(group: string, groups: TodoGroup[]) {
    if (group === "all") return "全部";
    if (group === "ungrouped") return "未分组";
    return groups.find((item) => item.uuid === group)?.name ?? "分组";
  }

  function groupColorValue(color: string): GroupColor {
    return groupColorOptions.some((option) => option.value === color)
      ? (color as GroupColor)
      : "yellow";
  }

  function markPanelInteraction() {
    if (isTauri()) {
      void todoApi.markPanelInteraction().catch(() => {});
    }
  }

  function applyTheme(nextTheme: Theme) {
    document.documentElement.dataset.theme = nextTheme;
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", nextTheme === "dark" ? "#1d1b18" : "#f6c94c");
  }

  function footerSyncLabel(
    kind: import("$lib/sync/autoSync").SyncStatusKind,
  ) {
    if (kind === "syncing") return "同步中";
    if (kind === "synced") return "已同步";
    if (kind === "offline") return "离线";
    if (kind === "conflict") return "有冲突";
    if (kind === "failed") return "同步失败";
    return "未同步";
  }

  async function addTodo() {
    const nextTitle = title.trim();
    if (!nextTitle || adding) return;
    const parsed = quickAddPreview ?? {
      title: nextTitle,
      schedule: null,
      label: "",
      groupName: null,
    };
    const groupUuid = groupUuidByName(parsed.groupName) ?? newTodoGroupUuid();

    adding = true;
    try {
      const created = await todos.add(parsed.title, groupUuid);
      if (parsed.schedule) {
        await todos.setSchedule(created.id, parsed.schedule);
      }
      title = "";
      quickAddParsingDisabledFor = "";
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

  async function pinTodo(todo: Todo, pinned: boolean) {
    try {
      await todos.setPinned(todo, pinned);
    } catch (error) {
      todos.reportError(error);
    }
  }

  async function createGroup() {
    const nextName = groupName.trim();
    if (!nextName || groupSaving) return;
    groupSaving = true;
    try {
      const group = await todos.addGroup(nextName);
      groupName = "";
      creatingGroup = false;
      setSelectedGroup(group.uuid);
    } catch (error) {
      todos.reportError(error);
    } finally {
      groupSaving = false;
    }
  }

  function disableQuickAddParsing() {
    quickAddParsingDisabledFor = title;
    inputElement?.focus();
  }

  function openGroupManager() {
    if (!selectedGroupObject) return;
    managingGroup = true;
    editingGroupName = selectedGroupObject.name;
    confirmingGroupDelete = false;
  }

  async function renameSelectedGroup() {
    if (!selectedGroupObject || groupSaving) return;
    const nextName = editingGroupName.trim();
    if (!nextName || nextName === selectedGroupObject.name) return;

    groupSaving = true;
    try {
      await todos.renameGroup(selectedGroupObject.uuid, nextName);
    } catch (error) {
      todos.reportError(error);
    } finally {
      groupSaving = false;
    }
  }

  async function updateSelectedGroupColor(color: GroupColor) {
    if (!selectedGroupObject || groupSaving || groupDeleting) return;
    if (groupColorValue(selectedGroupObject.color) === color) return;

    groupSaving = true;
    try {
      await todos.updateGroupColor(selectedGroupObject.uuid, color);
    } catch (error) {
      todos.reportError(error);
    } finally {
      groupSaving = false;
    }
  }

  async function deleteSelectedGroup() {
    if (!selectedGroupObject || groupDeleting) return;
    if (!confirmingGroupDelete) {
      confirmingGroupDelete = true;
      if (groupDeleteTimer) clearTimeout(groupDeleteTimer);
      groupDeleteTimer = setTimeout(() => {
        confirmingGroupDelete = false;
        groupDeleteTimer = null;
      }, 3000);
      return;
    }

    groupDeleting = true;
    try {
      await todos.deleteGroup(selectedGroupObject.uuid);
      setSelectedGroup("all");
      managingGroup = false;
    } catch (error) {
      todos.reportError(error);
    } finally {
      groupDeleting = false;
      confirmingGroupDelete = false;
    }
  }

  async function moveSelectedGroup(direction: -1 | 1) {
    if (!selectedGroupObject || groupSaving) return;
    const nextIndex = selectedGroupIndex + direction;
    if (selectedGroupIndex < 0 || nextIndex < 0 || nextIndex >= $todos.groups.length) {
      return;
    }
    const groups = [...$todos.groups];
    [groups[selectedGroupIndex], groups[nextIndex]] = [
      groups[nextIndex],
      groups[selectedGroupIndex],
    ];
    groupSaving = true;
    try {
      await todos.reorderGroups(groups.map((group) => group.uuid));
    } catch {
      // The store restores the previous group order and exposes the error.
    } finally {
      groupSaving = false;
    }
  }

  async function moveTodoToGroup(todo: Todo, groupUuid: string | null) {
    try {
      await todos.setGroup(todo, groupUuid);
    } catch (error) {
      todos.reportError(error);
      throw error;
    }
  }

  async function scheduleTodo(id: number, schedule: TodoScheduleInput) {
    try {
      await todos.setSchedule(id, schedule);
    } catch (error) {
      todos.reportError(error);
      throw error;
    }
  }

  async function snoozeTodo(todo: Todo, reminderAt: number) {
    try {
      await todos.setSchedule(todo.id, {
        due_date: todo.due_date,
        due_at: todo.due_at,
        reminder_at: reminderAt,
        repeat_rule: todo.repeat_rule,
      });
    } catch (error) {
      todos.reportError(error);
      throw error;
    }
  }

  async function deleteTodo(
    id: number,
    repeatScope: RepeatDeleteScope = "single",
  ) {
    try {
      undoTodos = await todos.remove(id, repeatScope);
      if (undoTimer) clearTimeout(undoTimer);
      undoTimer = setTimeout(() => {
        undoTodos = [];
        undoTimer = null;
      }, 5000);
    } catch (error) {
      todos.reportError(error);
    }
  }

  async function undoDelete() {
    if (undoTodos.length === 0) return;
    const todosToRestore = undoTodos;
    undoTodos = [];
    if (undoTimer) {
      clearTimeout(undoTimer);
      undoTimer = null;
    }
    try {
      for (const todo of todosToRestore) {
        await todos.restore(todo.id);
      }
    } catch (error) {
      undoTodos = todosToRestore;
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
    if (reorderDisabled && !canDragTodoToGroup(todo)) return;
    cancelDrag();
    draggedTodo = todo;
    dragPointerId = event.pointerId;
    dragGroupTarget = undefined;
    previewOrderIds = reorderDisabled
      ? null
      : $todos.items
          .filter(
            (item) =>
              item.completed === todo.completed && item.pinned === todo.pinned,
          )
          .map((item) => item.id);
    window.addEventListener("pointermove", moveDrag, true);
    window.addEventListener("pointerup", endDrag, true);
    window.addEventListener("pointercancel", cancelDrag, true);
  }

  function moveDrag(event: PointerEvent) {
    if (!draggedTodo || event.pointerId !== dragPointerId) return;
    event.preventDefault();
    dragGroupTarget = findGroupDropTarget(event.clientX, event.clientY);
    if (dragGroupTarget !== undefined) return;
    if (reorderDisabled) return;
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
        return todo &&
          todo.completed === source.completed &&
          todo.pinned === source.pinned
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
    const groupTarget = findGroupDropTarget(event.clientX, event.clientY);
    if (groupTarget !== undefined) {
      const todo = draggedTodo;
      removeDragListeners();
      resetDragState();
      try {
        await todos.setGroup(todo, groupTarget);
      } catch (error) {
        todos.reportError(error);
      }
      return;
    }

    if (!reorderDisabled) {
      updateDragTarget(event.clientY);
    }
    const orderedIds = previewOrderIds;
    const sourceCompleted = draggedTodo.completed;
    const sourcePinned = draggedTodo.pinned;
    removeDragListeners();
    if (!orderedIds) {
      resetDragState();
      return;
    }

    const currentIds = $todos.items
      .filter(
        (todo) =>
          todo.completed === sourceCompleted && todo.pinned === sourcePinned,
      )
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
    if (reorderDisabled) return;
    const group = $todos.items.filter(
      (item) =>
        item.completed === todo.completed && item.pinned === todo.pinned,
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
    dragGroupTarget = undefined;
  }

  function canDragTodoToGroup(todo: Todo) {
    return $todos.groups.length > 0 || todo.group_uuid !== null;
  }

  function findGroupDropTarget(clientX: number, clientY: number): GroupDropTarget | undefined {
    if (!draggedTodo) return undefined;
    const target = Array.from(
      document.querySelectorAll<HTMLElement>("[data-group-drop-target]"),
    ).find((element) => {
      const rect = element.getBoundingClientRect();
      return (
        clientX >= rect.left &&
        clientX <= rect.right &&
        clientY >= rect.top &&
        clientY <= rect.bottom
      );
    });
    if (!target) return undefined;

    const value = target.dataset.groupDropTarget;
    if (!value) return undefined;
    const groupUuid = value === "ungrouped" ? null : value;
    return groupUuid === draggedTodo.group_uuid ? undefined : groupUuid;
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

<svelte:window onpointerdown={markPanelInteraction} />

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
      placeholder={`准备完成什么？${groupLabel(selectedGroup, $todos.groups)} · 回车添加`}
      aria-label="新任务内容"
      autocomplete="off"
    />
    <button type="submit" disabled={!title.trim() || adding} aria-label="添加任务">
      {adding ? "…" : "+"}
    </button>
  </form>
  {#if quickAddPreview}
    <div class="quick-add-preview" role="status">
      <span>
        将创建“{quickAddPreview.title}”
        {#if quickAddPreview.label}，到期 {quickAddPreview.label}{/if}
        {#if quickAddPreview.groupName}，分组 {quickAddPreview.groupName}{/if}
      </span>
      <button type="button" onclick={disableQuickAddParsing}>不解析</button>
    </div>
  {/if}

  <section class="group-filter" aria-label="任务分组">
    <div class="group-scroll">
      <button
        class:active={selectedGroup === "all"}
        type="button"
        onclick={() => setSelectedGroup("all")}
      >
        全部
      </button>
      <button
        class:active={selectedGroup === "ungrouped"}
        class:drag-over={dragGroupTarget === null}
        data-group-drop-target="ungrouped"
        type="button"
        title="拖到这里移动到未分组"
        onclick={() => setSelectedGroup("ungrouped")}
      >
        未分组
      </button>
      {#each $todos.groups as group (group.uuid)}
        <button
          class="group-chip"
          class:active={selectedGroup === group.uuid}
          class:drag-over={dragGroupTarget === group.uuid}
          data-group-color={groupColorValue(group.color)}
          data-group-drop-target={group.uuid}
          type="button"
          title={`拖到这里移动到${group.name}`}
          onclick={() => setSelectedGroup(group.uuid)}
        >
          <span class="group-dot" aria-hidden="true"></span>
          {group.name}
        </button>
      {/each}
    </div>
    {#if creatingGroup}
      <form
        class="group-create"
        onsubmit={(event) => {
          event.preventDefault();
          void createGroup();
        }}
      >
        <input
          bind:value={groupName}
          maxlength="30"
          placeholder="新分组"
          aria-label="新分组名称"
          disabled={groupSaving}
        />
        <button type="submit" disabled={!groupName.trim() || groupSaving}>
          {groupSaving ? "…" : "保存"}
        </button>
        <button
          type="button"
          disabled={groupSaving}
          onclick={() => {
            creatingGroup = false;
            groupName = "";
          }}
        >
          取消
        </button>
      </form>
    {:else}
      {#if selectedGroupObject}
        <button
          class="group-manage-toggle"
          type="button"
          title="管理当前分组"
          onclick={openGroupManager}
        >
          管理
        </button>
      {/if}
      <button
        class="group-add"
        type="button"
        title="新建分组"
        onclick={() => (creatingGroup = true)}
      >
        +
      </button>
    {/if}
  </section>

  {#if managingGroup && selectedGroupObject}
    <form
      class="group-manager"
      onsubmit={(event) => {
        event.preventDefault();
        void renameSelectedGroup();
      }}
    >
      <div class="group-manager-name">
        <input
          bind:value={editingGroupName}
          maxlength="30"
          aria-label="分组名称"
          disabled={groupSaving || groupDeleting}
        />
        <button
          type="submit"
          disabled={
            groupSaving ||
            groupDeleting ||
            !editingGroupName.trim() ||
            editingGroupName.trim() === selectedGroupObject.name
          }
        >
          保存
        </button>
      </div>

      <div class="group-manager-tools">
        <div class="group-color-options" aria-label="分组颜色">
          {#each groupColorOptions as option}
            <button
              class="color-swatch"
              class:active={groupColorValue(selectedGroupObject.color) === option.value}
              data-group-color={option.value}
              type="button"
              title={`切换为${option.label}`}
              disabled={groupSaving || groupDeleting}
              onclick={() => void updateSelectedGroupColor(option.value)}
            >
              <span aria-hidden="true"></span>
            </button>
          {/each}
        </div>

        <div class="group-manager-actions">
          <button
            type="button"
            aria-label="上移分组"
            title="上移分组"
            disabled={groupSaving || groupDeleting || selectedGroupIndex <= 0}
            onclick={() => void moveSelectedGroup(-1)}
          >
            ↑
          </button>
          <button
            type="button"
            aria-label="下移分组"
            title="下移分组"
            disabled={
              groupSaving ||
              groupDeleting ||
              selectedGroupIndex < 0 ||
              selectedGroupIndex >= $todos.groups.length - 1
            }
            onclick={() => void moveSelectedGroup(1)}
          >
            ↓
          </button>
          <button
            class:confirming={confirmingGroupDelete}
            type="button"
            disabled={groupSaving || groupDeleting}
            onclick={() => void deleteSelectedGroup()}
          >
            {confirmingGroupDelete ? "确认删除？" : "删除"}
          </button>
          <button
            type="button"
            disabled={groupSaving || groupDeleting}
            onclick={() => {
              managingGroup = false;
              confirmingGroupDelete = false;
            }}
          >
            关闭
          </button>
        </div>
      </div>
    </form>
  {/if}

  {#if showSearch}
    <div class="todo-search">
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <circle cx="8.5" cy="8.5" r="4.5" />
        <path d="m12 12 4 4" />
      </svg>
      <input
        bind:this={searchInput}
        bind:value={searchQuery}
        type="search"
        maxlength="200"
        placeholder="搜索任务"
        aria-label="搜索任务"
        autocomplete="off"
        onkeydown={handleSearchKeydown}
      />
      {#if searchQuery}
        <button
          type="button"
          aria-label="清空搜索"
          title="清空搜索"
          onclick={() => {
            searchQuery = "";
            searchInput?.focus();
          }}
        >×</button>
      {/if}
    </div>
  {/if}

  <section class="summary">
    <div class="view-switch" aria-label="任务视图">
      <button
        class:active={listView === "all"}
        type="button"
        aria-pressed={listView === "all"}
        onclick={() => setListView("all")}
      >
        全部
      </button>
      <button
        class:active={listView === "today"}
        type="button"
        aria-pressed={listView === "today"}
        onclick={() => setListView("today")}
      >
        今天{todayCount > 0 ? ` ${todayCount}` : ""}
      </button>
    </div>
    <div class="summary-actions">
      <button
        class:active={showSearch}
        type="button"
        aria-label={showSearch ? "关闭搜索" : "搜索任务"}
        title={showSearch ? "关闭搜索" : "搜索任务"}
        onclick={() => void toggleSearch()}
      >
        搜索
      </button>
      {#if $completedCount > 0 && listView === "all"}
        <button
          class:active={!showCompleted}
          type="button"
          aria-pressed={!showCompleted}
          onclick={toggleCompletedVisibility}
        >
          {showCompleted ? "隐藏已完成" : "显示已完成"}
        </button>
      {/if}
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
    {:else if renderedTodos.length === 0}
      <div class="empty-state filtered-empty">
        <strong>
          {searchActive
            ? "没有找到匹配任务"
            : listView === "today"
              ? "今天还没有到期任务"
              : "已完成任务已隐藏"}
        </strong>
        <span>
          {searchActive
            ? "换个关键词试试"
            : listView === "today"
              ? "可以给任务设置今天或更早的到期日"
              : "需要时可以重新显示"}
        </span>
      </div>
    {:else}
      {#if $todos.error}
        <div class="inline-error" role="alert">{$todos.error}</div>
      {/if}
      {#each renderedTodos as todo (todo.id)}
        {@const group = renderedTodos.filter((item) => item.completed === todo.completed && item.pinned === todo.pinned)}
        {@const groupIndex = group.findIndex((item) => item.id === todo.id)}
        <div
          class="todo-row"
          animate:flip={{ duration: reorderAnimationDuration }}
        >
          <TodoItem
            {todo}
            onToggle={toggleTodo}
            onEdit={editTodo}
            onPin={pinTodo}
            onSchedule={scheduleTodo}
            onSnooze={snoozeTodo}
            groups={$todos.groups}
            onGroupChange={moveTodoToGroup}
            onDelete={deleteTodo}
            onMove={moveTodo}
            onDragStart={startDrag}
            canMoveUp={groupIndex > 0}
            canMoveDown={groupIndex < group.length - 1}
            isDragging={draggedTodo?.id === todo.id}
            isDragTarget={draggedTodo?.id === todo.id}
            dragDisabled={reorderDisabled && !canDragTodoToGroup(todo)}
            reorderDisabled={reorderDisabled}
          />
        </div>
      {/each}
    {/if}
  </section>

  <footer>
    <span>一步一点，不着急</span>
    <button
      class:syncing={$syncStatus.kind === "syncing"}
      class:sync-ok={$syncStatus.kind === "synced"}
      class:sync-problem={["offline", "conflict", "failed"].includes(
        $syncStatus.kind,
      )}
      class="footer-sync-status"
      type="button"
      title={$syncStatus.message}
      onclick={() => {
        showAbout = false;
        showDataManager = false;
        showSettings = true;
      }}
    >
      <span aria-hidden="true"></span>
      {footerSyncLabel($syncStatus.kind)}
    </button>
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
    defaultListViewMode={defaultListViewMode}
    onClose={() => (showSettings = false)}
    onChange={(settings) => (desktopSettings = settings)}
    onDefaultListViewChange={setDefaultListViewMode}
  />
{/if}

{#if undoTodos.length > 0}
  <div class="undo-toast" role="status">
    <span>
      {undoTodos.length === 1
        ? `已删除“${undoTodos[0].title}”`
        : `已删除 ${undoTodos.length} 个重复任务`}
    </span>
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
