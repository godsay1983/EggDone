<script lang="ts">
  import { isTauri } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
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
  import {
    initializeAutoSync,
    setAutoSyncForeground,
    syncStatus,
  } from "$lib/sync/autoSync";
  import type {
    RepeatDeleteScope,
    RepeatEditScope,
    Todo,
    TodoGroup,
  } from "$lib/types";
  import { movePreviewByPointer } from "$lib/utils/reorderPreview";
  import { isDueTodayOrOverdue, localDateString } from "$lib/utils/todoDates";
  import {
    filterTodos,
    type TodoListView,
  } from "$lib/utils/todoFilters";
  import { parseQuickAdd } from "$lib/utils/quickAdd";
  import {
    clearFocusTarget,
    FOCUS_SETTINGS_CHANGED_EVENT,
    getFocusDurations,
    getFocusTarget,
    saveFocusTarget,
    type FocusTarget,
    type FocusDurations,
    type FocusPhase,
  } from "$lib/utils/focusSettings";
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
  type FocusTodoPayload = { uuid: string };
  type QuadrantKey =
    | "importantUrgent"
    | "importantNotUrgent"
    | "normalUrgent"
    | "normalNotUrgent";
  type AgendaKey =
    | "overdue"
    | "today"
    | "tomorrow"
    | "week"
    | "later"
    | "unscheduled";

  const groupColorOptions: Array<{ value: GroupColor; label: string }> = [
    { value: "yellow", label: "蛋黄" },
    { value: "green", label: "薄荷" },
    { value: "blue", label: "晴空" },
    { value: "peach", label: "蜜桃" },
    { value: "lavender", label: "薰衣草" },
    { value: "gray", label: "米灰" },
  ];

  const quadrantDefinitions: Array<{
    key: QuadrantKey;
    title: string;
    subtitle: string;
    tone: string;
  }> = [
    {
      key: "importantUrgent",
      title: "立刻做",
      subtitle: "重要且紧急",
      tone: "warm",
    },
    {
      key: "importantNotUrgent",
      title: "安排做",
      subtitle: "重要不紧急",
      tone: "gold",
    },
    {
      key: "normalUrgent",
      title: "顺手做",
      subtitle: "不重要但紧急",
      tone: "blue",
    },
    {
      key: "normalNotUrgent",
      title: "有空再说",
      subtitle: "不重要不紧急",
      tone: "gray",
    },
  ];

  const agendaDefinitions: Array<{
    key: AgendaKey;
    title: string;
    subtitle: string;
  }> = [
    { key: "overdue", title: "逾期", subtitle: "已经过期，优先看一眼" },
    { key: "today", title: "今天", subtitle: "今天要处理" },
    { key: "tomorrow", title: "明天", subtitle: "提前安排" },
    { key: "week", title: "本周内", subtitle: "接下来几天" },
    { key: "later", title: "更晚", subtitle: "未来安排" },
    { key: "unscheduled", title: "无到期日", subtitle: "还没有明确时间" },
  ];
  let title = "";
  let quickAddParsingDisabledFor = "";
  let adding = false;
  let showAbout = false;
  let showDataManager = false;
  let showSettings = false;
  let showFocus = false;
  let focusDurations: FocusDurations = getFocusDurations();
  let focusPhase: FocusPhase = "focus";
  let focusRunning = false;
  let focusEndsAt: number | null = null;
  let focusRemainingMs = focusDurations.focus;
  let focusDisplayTime = "25:00";
  let focusDisplayHint = "开始一颗番茄，先把注意力放在眼前这一件事。";
  let focusDisplayPhase = "专注";
  let focusTarget: FocusTarget | null = getFocusTarget();
  let completingFocusTarget = false;
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
  let summaryMenuOpen = false;
  let searchQuery = "";
  let showCompleted = true;
  let listView: TodoListView = "all";
  let selectedQuadrant: QuadrantKey | "all" = "all";
  let selectedAgendaDate: string | null = null;
  let agendaWeekStartAt = startOfAgendaWeek();
  let agendaWeekVersion = 0;
  let agendaDatePickerOpen = false;
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
  let groupScrollElement: HTMLElement;
  let groupScrollOverflowing = false;
  let groupScrollCanMoveLeft = false;
  let groupScrollCanMoveRight = false;
  let groupScrollPointerId: number | null = null;
  let groupScrollStartX = 0;
  let groupScrollStartLeft = 0;
  let groupScrollDragging = false;
  let suppressGroupClick = false;
  let searchInput: HTMLInputElement;
  let summaryActionsElement: HTMLElement;
  let selectedTodoId: number | null = null;
  let editRequestTodoId: number | null = null;
  let editRequestSeq = 0;
  let batchMode = false;
  let batchSelectedIds = new Set<number>();
  let batchBusy = false;
  let batchMoveTarget = "";
  $: searchActive = searchQuery.trim().length > 0;
  $: reorderDisabled = searchActive || listView !== "all";
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
  $: batchSelectedTodos = renderedTodos.filter((todo) =>
    batchSelectedIds.has(todo.id),
  );
  $: batchIncompleteSelectedCount = batchSelectedTodos.filter(
    (todo) => !todo.completed,
  ).length;
  $: batchSelectionCount = batchSelectedTodos.length;
  $: if (
    selectedTodoId !== null &&
    !renderedTodos.some((todo) => todo.id === selectedTodoId)
  ) {
    selectedTodoId = renderedTodos[0]?.id ?? null;
  }
  $: if (batchSelectedIds.size > 0) {
    const visibleIds = new Set(renderedTodos.map((todo) => todo.id));
    const nextIds = new Set(
      [...batchSelectedIds].filter((id) => visibleIds.has(id)),
    );
    if (nextIds.size !== batchSelectedIds.size) {
      batchSelectedIds = nextIds;
    }
  }
  $: quickAddResult = parseQuickAdd(
    title,
    new Date(),
    $todos.groups.map((group) => group.name),
  );
  $: quickAddPreview =
    title.trim().length > 0 &&
    quickAddParsingDisabledFor !== title &&
    (quickAddResult.schedule !== null ||
      quickAddResult.groupName !== null ||
      quickAddResult.priority === 1)
      ? quickAddResult
      : null;
  $: focusDisplayPhase = focusPhase === "focus" ? "专注" : "休息";
  $: focusDisplayTime = formatFocusTime(focusRemainingMs);
  $: focusDisplayHint = focusRunning
    ? "保持当前节奏，结束后会切到下一阶段。"
    : focusRemainingMs === focusDurations[focusPhase]
      ? "开始一颗番茄，先把注意力放在眼前这一件事。"
      : "已经暂停，继续时会从当前剩余时间开始。";

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];
    let mounted = true;
    const groupResizeObserver = new ResizeObserver(updateGroupScrollState);
    const groupMutationObserver = new MutationObserver(() => {
      updateGroupScrollState();
      requestAnimationFrame(() => revealSelectedGroup(selectedGroup, false));
    });
    const focusInterval = window.setInterval(updateFocusTimer, 1000);
    window.addEventListener(FOCUS_SETTINGS_CHANGED_EVENT, refreshFocusDurations);
    window.addEventListener("storage", refreshFocusDurations);
    groupResizeObserver.observe(groupScrollElement);
    groupMutationObserver.observe(groupScrollElement, { childList: true });
    updateGroupScrollState();
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
    function handlePointerDown(event: PointerEvent) {
      if (
        summaryMenuOpen &&
        event.target instanceof Node &&
        !summaryActionsElement?.contains(event.target)
      ) {
        summaryMenuOpen = false;
      }
    }
    window.addEventListener("pointerdown", handlePointerDown, true);
    if (isTauri()) {
      void initializeAutoSync().then(async () => {
        const appWindow = getCurrentWindow();
        setAutoSyncForeground(await appWindow.isFocused());
        const unlistenFocus = await appWindow.onFocusChanged(({ payload }) => {
          setAutoSyncForeground(payload);
        });
        if (mounted) {
          unlisteners.push(unlistenFocus);
        } else {
          unlistenFocus();
        }
      });
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
      void listen<FocusTodoPayload>("focus-todo", (event) => {
        void focusTodoByUuid(event.payload.uuid);
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
      mounted = false;
      setAutoSyncForeground(false);
      window.removeEventListener("pointerdown", handlePointerDown, true);
      unlisteners.forEach((unlisten) => unlisten());
      if (undoTimer) clearTimeout(undoTimer);
      if (clearTimer) clearTimeout(clearTimer);
      if (groupDeleteTimer) clearTimeout(groupDeleteTimer);
      window.clearInterval(focusInterval);
      window.removeEventListener(
        FOCUS_SETTINGS_CHANGED_EVENT,
        refreshFocusDurations,
      );
      window.removeEventListener("storage", refreshFocusDurations);
      groupResizeObserver.disconnect();
      groupMutationObserver.disconnect();
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
    summaryMenuOpen = false;
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
    summaryMenuOpen = false;
    localStorage.setItem("eggdone-show-completed", String(showCompleted));
    clearBatchSelection();
    cancelDrag();
  }

  function setListView(view: TodoListView) {
    listView = view;
    if (view !== "quadrants") {
      selectedQuadrant = "all";
    }
    if (view !== "calendar") {
      selectedAgendaDate = null;
      agendaWeekStartAt = startOfAgendaWeek();
      agendaWeekVersion += 1;
      agendaDatePickerOpen = false;
    }
    summaryMenuOpen = false;
    localStorage.setItem(LAST_LIST_VIEW_KEY, view);
    selectedTodoId = null;
    clearBatchSelection();
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
    selectedTodoId = null;
    clearBatchSelection();
    cancelDrag();
    requestAnimationFrame(() => revealSelectedGroup(group));
  }

  function selectGroupFromScroll(group: string) {
    if (suppressGroupClick) return;
    setSelectedGroup(group);
  }

  function revealSelectedGroup(group: string, smooth = true) {
    if (!groupScrollElement) return;
    const buttons = groupScrollElement.querySelectorAll<HTMLButtonElement>(
      "[data-group-filter]",
    );
    const selectedButton = [...buttons].find(
      (button) => button.dataset.groupFilter === group,
    );
    selectedButton?.scrollIntoView({
      behavior: smooth ? "smooth" : "auto",
      block: "nearest",
      inline: "nearest",
    });
  }

  function updateGroupScrollState() {
    if (!groupScrollElement) return;
    const maxScrollLeft =
      groupScrollElement.scrollWidth - groupScrollElement.clientWidth;
    groupScrollOverflowing = maxScrollLeft > 2;
    groupScrollCanMoveLeft = groupScrollElement.scrollLeft > 2;
    groupScrollCanMoveRight =
      groupScrollElement.scrollLeft < maxScrollLeft - 2;
  }

  function scrollGroups(direction: -1 | 1) {
    groupScrollElement.scrollBy({
      left: direction * Math.max(120, groupScrollElement.clientWidth * 0.7),
      behavior: "smooth",
    });
  }

  function handleGroupScrollWheel(event: WheelEvent) {
    if (!groupScrollOverflowing) return;
    const delta =
      Math.abs(event.deltaX) > Math.abs(event.deltaY)
        ? event.deltaX
        : event.deltaY;
    if (delta === 0) return;
    event.preventDefault();
    groupScrollElement.scrollLeft += delta;
  }

  function handleGroupScrollPointerDown(event: PointerEvent) {
    if (event.button !== 0 || !groupScrollOverflowing) return;
    groupScrollPointerId = event.pointerId;
    groupScrollStartX = event.clientX;
    groupScrollStartLeft = groupScrollElement.scrollLeft;
    groupScrollDragging = false;
    suppressGroupClick = false;
  }

  function handleGroupScrollPointerMove(event: PointerEvent) {
    if (event.pointerId !== groupScrollPointerId) return;
    const distance = event.clientX - groupScrollStartX;
    if (!groupScrollDragging && Math.abs(distance) < 5) return;
    if (!groupScrollDragging) {
      groupScrollElement.setPointerCapture(event.pointerId);
    }
    groupScrollDragging = true;
    suppressGroupClick = true;
    event.preventDefault();
    groupScrollElement.scrollLeft = groupScrollStartLeft - distance;
  }

  function handleGroupScrollPointerLeave(event: PointerEvent) {
    if (
      event.pointerId === groupScrollPointerId &&
      !groupScrollDragging
    ) {
      groupScrollPointerId = null;
    }
  }

  function handleGroupScrollPointerUp(event: PointerEvent) {
    if (event.pointerId !== groupScrollPointerId) return;
    if (groupScrollElement.hasPointerCapture(event.pointerId)) {
      groupScrollElement.releasePointerCapture(event.pointerId);
    }
    groupScrollPointerId = null;
    groupScrollDragging = false;
    if (suppressGroupClick) {
      setTimeout(() => {
        suppressGroupClick = false;
      }, 0);
    }
  }

  async function focusTodoByUuid(uuid: string) {
    showAbout = false;
    showDataManager = false;
    showSettings = false;
    showSearch = false;
    searchQuery = "";
    showCompleted = true;
    localStorage.setItem("eggdone-show-completed", "true");
    setListView("all");
    setSelectedGroup("all");

    await tick();
    let todo = $todos.items.find((item) => item.uuid === uuid);
    if (!todo) {
      await todos.refresh();
      await tick();
      todo = $todos.items.find((item) => item.uuid === uuid);
    }
    if (!todo) {
      inputElement?.focus();
      return;
    }

    selectedTodoId = todo.id;
    requestAnimationFrame(() => {
      document
        .querySelector<HTMLElement>(`[data-todo-id="${todo.id}"]`)
        ?.scrollIntoView({ block: "center" });
      inputElement?.focus();
    });
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

  function markPanelInteraction(event: PointerEvent) {
    selectTodoFromPointer(event);
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

  function openFocusPanel() {
    clearFocusTarget();
    focusTarget = null;
    showAbout = false;
    showDataManager = false;
    showSettings = false;
    openFocusSurface();
  }

  function openFocusForTodo(todo: Todo) {
    focusTarget = { uuid: todo.uuid, title: todo.title };
    saveFocusTarget(focusTarget);
    showAbout = false;
    showDataManager = false;
    showSettings = false;
    openFocusSurface();
  }

  function openFocusSurface() {
    if (isTauri()) {
      void todoApi.openFocusWindow().catch((error) => {
        todos.reportError(error);
        showFocus = true;
      });
      return;
    }
    showFocus = true;
  }

  function refreshFocusDurations() {
    const previous = focusDurations;
    const next = getFocusDurations();
    focusDurations = next;
    if (
      !focusRunning &&
      focusEndsAt === null &&
      focusRemainingMs === previous[focusPhase]
    ) {
      focusRemainingMs = next[focusPhase];
    }
  }

  function startFocusSession(phase: FocusPhase = "focus") {
    refreshFocusDurations();
    focusPhase = phase;
    focusRemainingMs = focusDurations[phase];
    focusEndsAt = Date.now() + focusRemainingMs;
    focusRunning = true;
  }

  function updateFocusTimer() {
    if (!focusRunning || focusEndsAt === null) return;
    focusRemainingMs = Math.max(0, focusEndsAt - Date.now());
    if (focusRemainingMs > 0) return;
    const completedPhase = focusPhase;
    focusRunning = false;
    focusEndsAt = null;
    focusPhase = focusPhase === "focus" ? "break" : "focus";
    focusRemainingMs = focusDurations[focusPhase];
    if (isTauri()) {
      void todoApi.publishFocusNotification(completedPhase).catch(() => {});
    }
  }

  function toggleFocusRunning() {
    if (focusRunning) {
      updateFocusTimer();
      focusRunning = false;
      focusEndsAt = null;
      return;
    }
    focusEndsAt = Date.now() + focusRemainingMs;
    focusRunning = true;
  }

  function addFocusFiveMinutes() {
    focusRemainingMs += 5 * 60 * 1000;
    if (focusRunning) {
      focusEndsAt = Date.now() + focusRemainingMs;
    }
  }

  function skipFocusPhase() {
    startFocusSession(focusPhase === "focus" ? "break" : "focus");
  }

  function endFocusSession() {
    focusRunning = false;
    focusPhase = "focus";
    focusEndsAt = null;
    focusRemainingMs = focusDurations.focus;
    clearFocusTarget();
    focusTarget = null;
    showFocus = false;
  }

  async function completeFocusTarget() {
    if (!focusTarget || completingFocusTarget) return;
    const targetTodo = $todos.items.find((todo) => todo.uuid === focusTarget?.uuid);
    if (!targetTodo || targetTodo.completed) {
      endFocusSession();
      return;
    }
    completingFocusTarget = true;
    try {
      await todos.toggle(targetTodo);
      endFocusSession();
    } catch (error) {
      todos.reportError(error);
    } finally {
      completingFocusTarget = false;
    }
  }

  function formatFocusTime(milliseconds: number) {
    const totalSeconds = Math.ceil(milliseconds / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }

  async function addTodo() {
    const nextTitle = title.trim();
    if (!nextTitle || adding) return;
    const parsed = quickAddPreview ?? {
      title: nextTitle,
      schedule: null,
      label: "",
      groupName: null,
      priority: 0,
    };
    const groupUuid = groupUuidByName(parsed.groupName) ?? newTodoGroupUuid();

    adding = true;
    try {
      const created = await todos.add(parsed.title, groupUuid);
      if (parsed.schedule) {
        await todos.setSchedule(created.id, parsed.schedule);
      }
      if (parsed.priority === 1) {
        await todos.setPriority(created, 1);
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

  async function editTodo(
    id: number,
    nextTitle: string,
    repeatScope: RepeatEditScope = "single",
  ) {
    try {
      await todos.edit(id, nextTitle, repeatScope);
    } catch (error) {
      todos.reportError(error);
      throw error;
    }
  }

  async function noteTodo(
    id: number,
    note: string | null,
    repeatScope: RepeatEditScope = "single",
  ) {
    try {
      await todos.setNote(id, note, repeatScope);
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

  async function priorityTodo(todo: Todo, priority: number) {
    try {
      await todos.setPriority(todo, priority);
    } catch (error) {
      todos.reportError(error);
    }
  }

  function isUrgentTodo(todo: Todo) {
    return isDueTodayOrOverdue(todo);
  }

  function quadrantKey(todo: Todo): QuadrantKey {
    const important = todo.priority === 1;
    const urgent = isUrgentTodo(todo);
    if (important && urgent) return "importantUrgent";
    if (important) return "importantNotUrgent";
    if (urgent) return "normalUrgent";
    return "normalNotUrgent";
  }

  function quadrantTodos(key: QuadrantKey) {
    return renderedTodos.filter((todo) => quadrantKey(todo) === key);
  }

  function quadrantCount(key: QuadrantKey) {
    return filteredTodos.filter((todo) => quadrantKey(todo) === key).length;
  }

  function agendaKey(todo: Todo, now = new Date()): AgendaKey {
    const dueDate = todoAgendaDate(todo);
    if (dueDate === null) return "unscheduled";

    const today = localDateString(0, now);
    if (!todo.completed && todo.due_at !== null && todo.due_at < now.getTime()) {
      return "overdue";
    }
    if (!todo.completed && todo.due_date !== null && todo.due_date < today) {
      return "overdue";
    }
    if (dueDate === today) return "today";
    if (dueDate === localDateString(1, now)) return "tomorrow";
    if (dueDate <= localDateString(6, now)) return "week";
    return "later";
  }

  function agendaTodos(key: AgendaKey) {
    return renderedTodos.filter((todo) => agendaKey(todo) === key);
  }

  function agendaDateTodos(date: string) {
    return renderedTodos.filter((todo) => todoAgendaDate(todo) === date);
  }

  function startOfAgendaWeek(value = Date.now()) {
    const date = new Date(value);
    date.setHours(0, 0, 0, 0);
    date.setDate(date.getDate() - date.getDay());
    return date.getTime();
  }

  function agendaWeekDays(now = new Date(agendaWeekStartAt)) {
    return Array.from({ length: 7 }, (_, index) => {
      const date = new Date(now);
      date.setDate(now.getDate() + index);
      const dateKey = localDateString(0, date);
      return {
        dateKey,
        day: String(date.getDate()),
        label: agendaDayLabel(dateKey, date),
        count: renderedTodos.filter((todo) => todoAgendaDate(todo) === dateKey)
          .length,
      };
    });
  }

  function agendaDayLabel(dateKey: string, date: Date) {
    if (dateKey === localDateString(0)) return "今天";
    if (dateKey === localDateString(1)) return "明天";
    return ["周日", "周一", "周二", "周三", "周四", "周五", "周六"][
      date.getDay()
    ];
  }

  function selectedAgendaLabel() {
    const dateKey = selectedAgendaDate ?? localDateString(0);
    if (dateKey === localDateString(0)) return "今天";
    if (dateKey === localDateString(1)) return "明天";
    return dateKey.slice(5).replace("-", "/");
  }

  function selectAgendaDate(dateKey: string) {
    selectedAgendaDate = dateKey;
    agendaDatePickerOpen = false;
  }

  function shiftAgendaWeek(offsetDays: number) {
    const date = new Date(startOfAgendaWeek(agendaWeekStartAt));
    date.setDate(date.getDate() + offsetDays);
    agendaWeekStartAt = date.getTime();
    selectedAgendaDate = localDateString(0, date);
    agendaWeekVersion += 1;
    agendaDatePickerOpen = false;
  }

  function jumpAgendaToday() {
    agendaWeekStartAt = startOfAgendaWeek();
    selectedAgendaDate = localDateString(0);
    agendaWeekVersion += 1;
    agendaDatePickerOpen = false;
  }

  function setAgendaDateFromPicker(dateKey: string) {
    if (!dateKey) return;
    selectedAgendaDate = dateKey;
    agendaWeekStartAt = startOfAgendaWeek(
      new Date(`${dateKey}T00:00:00`).getTime(),
    );
    agendaWeekVersion += 1;
    agendaDatePickerOpen = false;
  }

  function todoAgendaDate(todo: Todo): string | null {
    if (todo.due_date !== null) return todo.due_date;
    if (todo.due_at !== null) return localDateString(0, new Date(todo.due_at));
    return null;
  }

  function selectTodo(id: number) {
    selectedTodoId = id;
  }

  function selectTodoFromPointer(event: PointerEvent) {
    if (event.button !== 0) return;
    if (!(event.target instanceof HTMLElement)) return;
    const item = event.target.closest<HTMLElement>("[data-todo-id]");
    const id = Number(item?.dataset.todoId);
    if (Number.isFinite(id)) selectedTodoId = id;
  }

  function moveKeyboardSelection(direction: -1 | 1) {
    if (renderedTodos.length === 0) return;
    const currentIndex = renderedTodos.findIndex(
      (todo) => todo.id === selectedTodoId,
    );
    const nextIndex =
      currentIndex === -1
        ? direction > 0
          ? 0
          : renderedTodos.length - 1
        : Math.min(
            Math.max(currentIndex + direction, 0),
            renderedTodos.length - 1,
          );
    selectedTodoId = renderedTodos[nextIndex].id;
    requestAnimationFrame(() => {
      document
        .querySelector<HTMLElement>(`[data-todo-id="${selectedTodoId}"]`)
        ?.scrollIntoView({ block: "nearest" });
    });
  }

  function requestEditSelectedTodo() {
    if (selectedTodoId === null) return;
    editRequestTodoId = selectedTodoId;
    editRequestSeq += 1;
  }

  function handlePanelKeydown(event: KeyboardEvent) {
    if (shouldIgnoreKeyboardNavigation(event)) return;
    if (event.key === "ArrowDown" || event.key === "j") {
      event.preventDefault();
      moveKeyboardSelection(1);
      return;
    }
    if (event.key === "ArrowUp" || event.key === "k") {
      event.preventDefault();
      moveKeyboardSelection(-1);
      return;
    }
    if (event.key === " ") {
      if (selectedTodoId === null) return;
      const todo = renderedTodos.find((item) => item.id === selectedTodoId);
      if (!todo) return;
      event.preventDefault();
      void toggleTodo(todo);
      return;
    }
    if (event.key === "Enter") {
      if (selectedTodoId === null) return;
      event.preventDefault();
      requestEditSelectedTodo();
      return;
    }
    if (event.key === "Escape" && selectedTodoId !== null) {
      selectedTodoId = null;
    }
  }

  function shouldIgnoreKeyboardNavigation(event: KeyboardEvent) {
    if (event.altKey || event.ctrlKey || event.metaKey) return true;
    if (
      showAbout ||
      showDataManager ||
      showSettings ||
      managingGroup ||
      creatingGroup ||
      draggedTodo
    ) {
      return true;
    }
    const target = event.target;
    if (!(target instanceof HTMLElement)) return false;
    const tag = target.tagName.toLowerCase();
    return (
      tag === "input" ||
      tag === "textarea" ||
      tag === "select" ||
      target.isContentEditable
    );
  }

  function toggleBatchMode() {
    batchMode = !batchMode;
    summaryMenuOpen = false;
    clearBatchSelection();
    if (batchMode) {
      showAbout = false;
      showDataManager = false;
      showSettings = false;
    }
  }

  function clearBatchSelection() {
    batchSelectedIds = new Set();
  }

  function toggleBatchTodo(todo: Todo, selected: boolean) {
    const nextIds = new Set(batchSelectedIds);
    if (selected) {
      nextIds.add(todo.id);
    } else {
      nextIds.delete(todo.id);
    }
    batchSelectedIds = nextIds;
  }

  function selectAllRenderedTodos() {
    batchSelectedIds = new Set(renderedTodos.map((todo) => todo.id));
  }

  function selectedBatchIds() {
    return batchSelectedTodos.map((todo) => todo.id);
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

  async function moveTodoToGroup(
    todo: Todo,
    groupUuid: string | null,
    repeatScope: RepeatEditScope = "single",
  ) {
    try {
      await todos.setGroup(todo, groupUuid, repeatScope);
    } catch (error) {
      todos.reportError(error);
      throw error;
    }
  }

  async function scheduleTodo(
    id: number,
    schedule: TodoScheduleInput,
    repeatScope: RepeatEditScope = "single",
  ) {
    try {
      await todos.setSchedule(id, schedule, repeatScope);
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
    summaryMenuOpen = false;
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

  async function archiveCompleted() {
    summaryMenuOpen = false;
    try {
      await todos.archiveCompleted();
    } catch (error) {
      todos.reportError(error);
    }
  }

  async function completeSelectedTodos() {
    if (batchBusy || batchIncompleteSelectedCount === 0) return;
    batchBusy = true;
    try {
      await todos.completeMany(selectedBatchIds());
      clearBatchSelection();
    } catch (error) {
      todos.reportError(error);
    } finally {
      batchBusy = false;
    }
  }

  async function moveSelectedTodos(groupUuid: string | null) {
    if (batchBusy || batchSelectionCount === 0) return;
    batchBusy = true;
    try {
      await todos.moveManyToGroup(selectedBatchIds(), groupUuid);
      clearBatchSelection();
    } catch (error) {
      todos.reportError(error);
    } finally {
      batchBusy = false;
    }
  }

  async function deleteSelectedTodos() {
    if (batchBusy || batchSelectionCount === 0) return;
    batchBusy = true;
    try {
      undoTodos = await todos.removeMany(selectedBatchIds());
      clearBatchSelection();
      if (undoTimer) clearTimeout(undoTimer);
      undoTimer = setTimeout(() => {
        undoTodos = [];
        undoTimer = null;
      }, 5000);
    } catch (error) {
      todos.reportError(error);
    } finally {
      batchBusy = false;
    }
  }

  function startDrag(todo: Todo, event: PointerEvent) {
    if (batchMode) return;
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

<svelte:window
  onpointerdown={markPanelInteraction}
  onkeydown={handlePanelKeydown}
/>

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
        class="focus-button"
        type="button"
        aria-label="专注"
        title="专注"
        onclick={openFocusPanel}
      >
        <svg viewBox="0 0 20 20" aria-hidden="true">
          <circle cx="10" cy="10" r="6.2" />
          <path d="M10 6.4v3.8l2.4 1.4" />
        </svg>
      </button>

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
        {#if quickAddPreview.priority === 1}，标记重要{/if}
      </span>
      <button type="button" onclick={disableQuickAddParsing}>不解析</button>
    </div>
  {/if}

  <section class="group-filter" aria-label="任务分组">
    {#if groupScrollOverflowing}
      <button
        class="group-scroll-control"
        type="button"
        aria-label="向左浏览分组"
        title="向左浏览分组"
        disabled={!groupScrollCanMoveLeft}
        onclick={() => scrollGroups(-1)}
      >
        ‹
      </button>
    {/if}
    <div
      bind:this={groupScrollElement}
      class="group-scroll"
      class:overflowing={groupScrollOverflowing}
      class:dragging={groupScrollDragging}
      role="group"
      aria-label="可横向浏览的任务分组"
      onscroll={updateGroupScrollState}
      onwheel={handleGroupScrollWheel}
      onpointerdown={handleGroupScrollPointerDown}
      onpointermove={handleGroupScrollPointerMove}
      onpointerup={handleGroupScrollPointerUp}
      onpointercancel={handleGroupScrollPointerUp}
      onpointerleave={handleGroupScrollPointerLeave}
    >
      <button
        class:active={selectedGroup === "all"}
        data-group-filter="all"
        type="button"
        onclick={() => selectGroupFromScroll("all")}
      >
        全部
      </button>
      <button
        class:active={selectedGroup === "ungrouped"}
        class:drag-over={dragGroupTarget === null}
        data-group-filter="ungrouped"
        data-group-drop-target="ungrouped"
        type="button"
        title="拖到这里移动到未分组"
        onclick={() => selectGroupFromScroll("ungrouped")}
      >
        未分组
      </button>
      {#each $todos.groups as group (group.uuid)}
        <button
          class="group-chip"
          class:active={selectedGroup === group.uuid}
          class:drag-over={dragGroupTarget === group.uuid}
          data-group-color={groupColorValue(group.color)}
          data-group-filter={group.uuid}
          data-group-drop-target={group.uuid}
          type="button"
          title={`拖到这里移动到${group.name}`}
          onclick={() => selectGroupFromScroll(group.uuid)}
        >
          <span class="group-dot" aria-hidden="true"></span>
          {group.name}
        </button>
      {/each}
    </div>
    {#if groupScrollOverflowing}
      <button
        class="group-scroll-control"
        type="button"
        aria-label="向右浏览分组"
        title="向右浏览分组"
        disabled={!groupScrollCanMoveRight}
        onclick={() => scrollGroups(1)}
      >
        ›
      </button>
    {/if}
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
      <button
        class:active={listView === "quadrants"}
        type="button"
        aria-pressed={listView === "quadrants"}
        onclick={() => setListView("quadrants")}
      >
        四象限
      </button>
      <button
        class:active={listView === "calendar"}
        type="button"
        aria-pressed={listView === "calendar"}
        onclick={() => setListView("calendar")}
      >
        日历
      </button>
    </div>
    <div bind:this={summaryActionsElement} class="summary-actions">
      <span class="count">{$remainingCount} 项未完成</span>
      <button
        class:active={summaryMenuOpen || showSearch || batchMode}
        class="summary-menu-button"
        type="button"
        aria-label="打开更多操作"
        aria-haspopup="menu"
        aria-expanded={summaryMenuOpen}
        onclick={() => (summaryMenuOpen = !summaryMenuOpen)}
      >
        更多
      </button>
      {#if summaryMenuOpen}
        <div class="summary-menu" role="menu">
          <button
            class:active={showSearch}
            type="button"
            role="menuitem"
            onclick={() => void toggleSearch()}
          >
            {showSearch ? "关闭搜索" : "搜索任务"}
          </button>
          {#if $completedCount > 0 && listView === "all"}
            <button
              class:active={!showCompleted}
              type="button"
              role="menuitem"
              onclick={toggleCompletedVisibility}
            >
              {showCompleted ? "隐藏已完成" : "显示已完成"}
            </button>
          {/if}
          {#if $completedCount > 0}
            <button
              type="button"
              role="menuitem"
              onclick={() => void archiveCompleted()}
            >
              归档已完成
            </button>
            <button
              class:confirming={confirmingClear}
              type="button"
              role="menuitem"
              onclick={requestClearCompleted}
            >
              {confirmingClear ? "确认清除？" : "清除已完成"}
            </button>
          {/if}
          {#if renderedTodos.length > 0}
            <button
              class:active={batchMode}
              type="button"
              role="menuitem"
              onclick={toggleBatchMode}
            >
              {batchMode ? "退出批量" : "批量操作"}
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </section>

  {#if batchMode && renderedTodos.length > 0}
    <section class="batch-toolbar" aria-label="批量操作">
      <span>{batchSelectionCount > 0 ? `已选 ${batchSelectionCount}` : "选择任务"}</span>
      <button
        type="button"
        disabled={batchBusy || batchSelectionCount === renderedTodos.length}
        onclick={selectAllRenderedTodos}
      >
        全选
      </button>
      <button
        type="button"
        disabled={batchBusy || batchSelectionCount === 0}
        onclick={clearBatchSelection}
      >
        清空
      </button>
      <button
        type="button"
        disabled={batchBusy || batchIncompleteSelectedCount === 0}
        onclick={() => void completeSelectedTodos()}
      >
        完成
      </button>
      <label class:placeholder={batchMoveTarget === ""} aria-label="批量移动到分组">
        <select
          bind:value={batchMoveTarget}
          disabled={batchBusy || batchSelectionCount === 0}
          onchange={(event) => {
            const value = event.currentTarget.value;
            if (!value) return;
            batchMoveTarget = "";
            void moveSelectedTodos(value === "ungrouped" ? null : value);
          }}
        >
          <option value="" disabled hidden></option>
          <option value="ungrouped">未分组</option>
          {#each $todos.groups as group (group.uuid)}
            <option value={group.uuid}>{group.name}</option>
          {/each}
        </select>
      </label>
      <button
        class="danger"
        type="button"
        disabled={batchBusy || batchSelectionCount === 0}
        onclick={() => void deleteSelectedTodos()}
      >
        删除
      </button>
    </section>
  {/if}

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
              : listView === "quadrants"
                ? "四象限里还没有匹配任务"
                : listView === "calendar"
                  ? "日程里还没有匹配任务"
              : "已完成任务已隐藏"}
        </strong>
        <span>
          {searchActive
            ? "换个关键词试试"
            : listView === "today"
              ? "可以给任务设置今天或更早的到期日"
              : listView === "quadrants"
                ? "可以先新增任务或调整筛选条件"
                : listView === "calendar"
                  ? "给任务设置到期时间后会自动归入日程"
              : "需要时可以重新显示"}
        </span>
      </div>
    {:else}
      {#if $todos.error}
        <div class="inline-error" role="alert">{$todos.error}</div>
      {/if}
      {#if listView === "quadrants"}
        <div class="quadrant-overview" aria-label="四象限概览">
          {#each quadrantDefinitions as quadrant (quadrant.key)}
            <button
              class:active={selectedQuadrant === quadrant.key}
              class={`quadrant-card ${quadrant.tone}`}
              type="button"
              aria-pressed={selectedQuadrant === quadrant.key}
              onclick={() =>
                (selectedQuadrant =
                  selectedQuadrant === quadrant.key ? "all" : quadrant.key)}
            >
              <span>{quadrant.title}</span>
              <strong>{quadrantCount(quadrant.key)}</strong>
              <small>{quadrant.subtitle}</small>
            </button>
          {/each}
        </div>
        {#if selectedQuadrant !== "all"}
          <button
            class="quadrant-reset"
            type="button"
            onclick={() => (selectedQuadrant = "all")}
          >
            显示全部象限
          </button>
        {/if}
        <div class="quadrant-sections">
          {#each quadrantDefinitions.filter((quadrant) => selectedQuadrant === "all" || selectedQuadrant === quadrant.key) as quadrant (quadrant.key)}
            {@const sectionTodos = quadrantTodos(quadrant.key)}
            <section class={`quadrant-section ${quadrant.tone}`}>
              <header>
                <span class="quadrant-dot" aria-hidden="true"></span>
                <div>
                  <strong>{quadrant.title}</strong>
                  <span>{quadrant.subtitle}</span>
                </div>
                <small>{sectionTodos.length}</small>
              </header>
              {#if sectionTodos.length === 0}
                <div class="quadrant-empty">这里暂时没有任务</div>
              {:else}
                {#each sectionTodos as todo (todo.id)}
                  {@const group = sectionTodos.filter((item) => item.completed === todo.completed && item.pinned === todo.pinned)}
                  {@const groupIndex = group.findIndex((item) => item.id === todo.id)}
                  <div
                    class:selected={selectedTodoId === todo.id}
                    class="todo-row"
                    animate:flip={{ duration: reorderAnimationDuration }}
                  >
                    <TodoItem
                      {todo}
                      onToggle={toggleTodo}
                      onEdit={editTodo}
                      onNote={noteTodo}
                      onPin={pinTodo}
                      onPriority={priorityTodo}
                      onFocus={openFocusForTodo}
                      onSchedule={scheduleTodo}
                      onSnooze={snoozeTodo}
                      groups={$todos.groups}
                      onGroupChange={moveTodoToGroup}
                      onDelete={deleteTodo}
                      onMove={moveTodo}
                      onDragStart={startDrag}
                      batchMode={batchMode}
                      batchSelected={batchSelectedIds.has(todo.id)}
                      onBatchSelect={toggleBatchTodo}
                      canMoveUp={groupIndex > 0}
                      canMoveDown={groupIndex < group.length - 1}
                      isDragging={draggedTodo?.id === todo.id}
                      isDragTarget={draggedTodo?.id === todo.id}
                      dragDisabled={true}
                      reorderDisabled={true}
                      editRequest={
                        editRequestTodoId === todo.id ? editRequestSeq : 0
                      }
                    />
                  </div>
                {/each}
              {/if}
            </section>
          {/each}
        </div>
      {:else if listView === "calendar"}
        <section class="agenda-nav" aria-label="日程日期">
          <div class="agenda-week-actions">
            <button type="button" onclick={() => shiftAgendaWeek(-7)}>
              上一周
            </button>
            <button type="button" onclick={jumpAgendaToday}>今天</button>
            <button type="button" onclick={() => shiftAgendaWeek(7)}>
              下一周
            </button>
          </div>
          {#key `${agendaWeekStartAt}-${agendaWeekVersion}`}
            <div class="agenda-week-strip">
              {#each agendaWeekDays() as day (day.dateKey)}
                <button
                  class:active={selectedAgendaDate === day.dateKey}
                  class:today={day.dateKey === localDateString(0)}
                  type="button"
                  aria-pressed={selectedAgendaDate === day.dateKey}
                  onclick={() => selectAgendaDate(day.dateKey)}
                >
                  <span>{day.label}</span>
                  <strong>{day.day}</strong>
                  <small class:visible={day.count > 0}>{day.count}</small>
                </button>
              {/each}
            </div>
          {/key}
          <div class="agenda-jump">
            <button
              type="button"
              onclick={() => (agendaDatePickerOpen = !agendaDatePickerOpen)}
            >
              跳转日期
            </button>
            {#if selectedAgendaDate}
              <button
                class="plain"
                type="button"
                onclick={() => (selectedAgendaDate = null)}
              >
                显示全部
              </button>
            {/if}
          </div>
          {#if agendaDatePickerOpen}
            <input
              class="agenda-date-picker"
              type="date"
              value={selectedAgendaDate ?? localDateString(0)}
              onchange={(event) => {
                setAgendaDateFromPicker(event.currentTarget.value);
              }}
            />
          {/if}
        </section>
        <div class="agenda-sections">
          {#if selectedAgendaDate}
            {#key selectedAgendaDate}
              {@const sectionTodos = agendaDateTodos(selectedAgendaDate)}
              <section class="agenda-section selected-date">
                <header>
                  <div>
                    <strong>{selectedAgendaLabel()}</strong>
                    <span>选中日期任务</span>
                  </div>
                  <small>{sectionTodos.length}</small>
                </header>
                {#if sectionTodos.length === 0}
                  <div class="agenda-empty">这一天还没有任务</div>
                {:else}
                  {#each sectionTodos as todo (todo.id)}
                    {@const group = sectionTodos.filter((item) => item.completed === todo.completed && item.pinned === todo.pinned)}
                    {@const groupIndex = group.findIndex((item) => item.id === todo.id)}
                    <div
                      class:selected={selectedTodoId === todo.id}
                      class="todo-row"
                      animate:flip={{ duration: reorderAnimationDuration }}
                    >
                      <TodoItem
                        {todo}
                        onToggle={toggleTodo}
                        onEdit={editTodo}
                        onNote={noteTodo}
                        onPin={pinTodo}
                        onPriority={priorityTodo}
                        onFocus={openFocusForTodo}
                        onSchedule={scheduleTodo}
                        onSnooze={snoozeTodo}
                        groups={$todos.groups}
                        onGroupChange={moveTodoToGroup}
                        onDelete={deleteTodo}
                        onMove={moveTodo}
                        onDragStart={startDrag}
                        batchMode={batchMode}
                        batchSelected={batchSelectedIds.has(todo.id)}
                        onBatchSelect={toggleBatchTodo}
                        canMoveUp={groupIndex > 0}
                        canMoveDown={groupIndex < group.length - 1}
                        isDragging={draggedTodo?.id === todo.id}
                        isDragTarget={draggedTodo?.id === todo.id}
                        dragDisabled={true}
                        reorderDisabled={true}
                        editRequest={
                          editRequestTodoId === todo.id ? editRequestSeq : 0
                        }
                      />
                    </div>
                  {/each}
                {/if}
              </section>
            {/key}
          {:else}
            {#each agendaDefinitions as section (section.key)}
            {@const sectionTodos = agendaTodos(section.key)}
            {#if sectionTodos.length > 0}
              <section class={`agenda-section ${section.key}`}>
                <header>
                  <div>
                    <strong>{section.title}</strong>
                    <span>{section.subtitle}</span>
                  </div>
                  <small>{sectionTodos.length}</small>
                </header>
                {#each sectionTodos as todo (todo.id)}
                  {@const group = sectionTodos.filter((item) => item.completed === todo.completed && item.pinned === todo.pinned)}
                  {@const groupIndex = group.findIndex((item) => item.id === todo.id)}
                  <div
                    class:selected={selectedTodoId === todo.id}
                    class="todo-row"
                    animate:flip={{ duration: reorderAnimationDuration }}
                  >
                    <TodoItem
                      {todo}
                      onToggle={toggleTodo}
                      onEdit={editTodo}
                      onNote={noteTodo}
                      onPin={pinTodo}
                      onPriority={priorityTodo}
                      onFocus={openFocusForTodo}
                      onSchedule={scheduleTodo}
                      onSnooze={snoozeTodo}
                      groups={$todos.groups}
                      onGroupChange={moveTodoToGroup}
                      onDelete={deleteTodo}
                      onMove={moveTodo}
                      onDragStart={startDrag}
                      batchMode={batchMode}
                      batchSelected={batchSelectedIds.has(todo.id)}
                      onBatchSelect={toggleBatchTodo}
                      canMoveUp={groupIndex > 0}
                      canMoveDown={groupIndex < group.length - 1}
                      isDragging={draggedTodo?.id === todo.id}
                      isDragTarget={draggedTodo?.id === todo.id}
                      dragDisabled={true}
                      reorderDisabled={true}
                      editRequest={
                        editRequestTodoId === todo.id ? editRequestSeq : 0
                      }
                    />
                  </div>
                {/each}
              </section>
            {/if}
            {/each}
          {/if}
        </div>
      {:else}
      {#each renderedTodos as todo (todo.id)}
        {@const group = renderedTodos.filter((item) => item.completed === todo.completed && item.pinned === todo.pinned)}
        {@const groupIndex = group.findIndex((item) => item.id === todo.id)}
        <div
          class:selected={selectedTodoId === todo.id}
          class="todo-row"
          animate:flip={{ duration: reorderAnimationDuration }}
        >
          <TodoItem
            {todo}
            onToggle={toggleTodo}
            onEdit={editTodo}
            onNote={noteTodo}
            onPin={pinTodo}
            onPriority={priorityTodo}
            onFocus={openFocusForTodo}
            onSchedule={scheduleTodo}
            onSnooze={snoozeTodo}
            groups={$todos.groups}
            onGroupChange={moveTodoToGroup}
            onDelete={deleteTodo}
            onMove={moveTodo}
            onDragStart={startDrag}
            batchMode={batchMode}
            batchSelected={batchSelectedIds.has(todo.id)}
            onBatchSelect={toggleBatchTodo}
            canMoveUp={groupIndex > 0}
            canMoveDown={groupIndex < group.length - 1}
            isDragging={draggedTodo?.id === todo.id}
            isDragTarget={draggedTodo?.id === todo.id}
            dragDisabled={batchMode || (reorderDisabled && !canDragTodoToGroup(todo))}
            reorderDisabled={reorderDisabled}
            editRequest={
              editRequestTodoId === todo.id ? editRequestSeq : 0
            }
          />
        </div>
      {/each}
      {/if}
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

{#if showFocus}
  <div class="focus-backdrop">
    <button class="focus-dismiss" type="button" aria-label="关闭专注面板" onclick={() => (showFocus = false)}></button>
    <div class="focus-card" role="dialog" aria-modal="true" aria-labelledby="focus-title">
      <header>
        <div>
          <p>番茄钟</p>
          <h2 id="focus-title">{focusDisplayPhase}</h2>
        </div>
        <button type="button" aria-label="关闭" onclick={() => (showFocus = false)}>×</button>
      </header>

      <img class="focus-illustration" src="/focus-illustration.png" alt="" aria-hidden="true" />

      <strong class="focus-time">{focusDisplayTime}</strong>
      {#if focusTarget}
        <div class="focus-target-row">
          <small class="focus-target">正在专注：{focusTarget.title}</small>
          <button type="button" onclick={() => void completeFocusTarget()} disabled={completingFocusTarget}>
            {completingFocusTarget ? "完成中" : "完成"}
          </button>
        </div>
      {/if}
      <p class="focus-copy">{focusDisplayHint}</p>

      <div class="focus-actions">
        <button type="button" class="primary" onclick={() => {
          if (focusRemainingMs === focusDurations[focusPhase] && !focusRunning) {
            startFocusSession(focusPhase);
          } else {
            toggleFocusRunning();
          }
        }}>{focusRunning ? "暂停" : "开始"}</button>
        <button type="button" onclick={addFocusFiveMinutes}>+5 分钟</button>
        <button type="button" onclick={skipFocusPhase}>跳过</button>
        <button type="button" onclick={endFocusSession}>结束</button>
      </div>
    </div>
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
