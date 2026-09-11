import { derived, writable } from "svelte/store";

import { noteApi } from "$lib/api/noteApi";
import { localizedErrorMessage } from "$lib/i18n/errors";
import { scheduleAutoSync } from "$lib/sync/autoSync";
import type { Note, NoteColor } from "$lib/types";

const AUTO_SAVE_DELAY_MS = 600;

interface PendingNoteSave {
  note: Note;
  title: string;
  content: string;
}

export interface NoteState {
  items: Note[];
  loading: boolean;
  saving: boolean;
  dirty: boolean;
  dirtySince: number | null;
  searchQuery: string;
  error: string | null;
}

const initialState: NoteState = {
  items: [],
  loading: true,
  saving: false,
  dirty: false,
  dirtySince: null,
  searchQuery: "",
  error: null,
};

export function createNoteStore(
  api = noteApi,
  onChanged: () => void = scheduleAutoSync,
  autoSaveDelayMs = AUTO_SAVE_DELAY_MS,
) {
  const store = writable<NoteState>({ ...initialState });
  const { subscribe, update } = store;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const pendingSaves = new Map<string, PendingNoteSave>();
  let saveInFlight: Promise<Note | null> | null = null;

  function markChanged() {
    update((state) => ({
      ...state,
      dirty: true,
      dirtySince: Date.now(),
      error: null,
    }));
    onChanged();
  }

  function replaceNote(note: Note) {
    update((state) => ({
      ...state,
      items: state.items
        .map((item) => (item.uuid === note.uuid ? note : item))
        .sort(sortNotes),
      error: null,
    }));
  }

  async function persistPendingSave(): Promise<Note | null> {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (saveInFlight) return saveInFlight;
    if (pendingSaves.size === 0) {
      update((state) => ({ ...state, saving: false }));
      return null;
    }

    update((state) => ({ ...state, saving: true }));
    const task = drainPendingSaves();
    saveInFlight = task;
    try {
      return await task;
    } finally {
      saveInFlight = null;
      update((state) => ({ ...state, saving: false }));
    }
  }

  async function drainPendingSaves(): Promise<Note | null> {
    let saved: Note | null = null;
    try {
      while (pendingSaves.size > 0) {
        const pending = pendingSaves.values().next().value!;
        saved = await api.update(pending.note.uuid, pending.title, pending.content);
        // A newer edit replaces the queued snapshot, not the in-flight write.
        if (pendingSaves.get(pending.note.uuid) === pending) pendingSaves.delete(pending.note.uuid);
        replaceNote(saved);
        markChanged();
      }
      return saved;
    } catch (error) {
      if (saveTimer) clearTimeout(saveTimer);
      saveTimer = null;
      update((state) => ({ ...state, error: errorMessage(error) }));
      throw error;
    }
  }

  return {
    hasPendingSave: () => pendingSaves.size > 0,
    clearSaveError() {
      if (pendingSaves.size === 0) update((state) => ({ ...state, error: null }));
    },
    subscribe,

    async load() {
      update((state) => ({ ...state, loading: true, error: pendingSaves.size > 0 ? state.error : null }));
      try {
        const items = await api.list();
        update((state) => ({
          ...state,
          items: [...items].sort(sortNotes),
          loading: false,
          error: pendingSaves.size > 0 ? state.error : null,
        }));
      } catch (error) {
        update((state) => ({
          ...state,
          loading: false,
          error: errorMessage(error),
        }));
      }
    },

    async refresh() {
      try {
        const items = await api.list();
        update((state) => ({
          ...state,
          items: [...items].sort(sortNotes),
          error: pendingSaves.size > 0 ? state.error : null,
        }));
      } catch (error) {
        update((state) => ({ ...state, error: errorMessage(error) }));
      }
    },

    async add(title: string, content: string, color: NoteColor = "default") {
      try {
        const note = await api.create(title, content, color);
        update((state) => ({
          ...state,
          items: [note, ...state.items].sort(sortNotes),
          error: null,
        }));
        markChanged();
        return note;
      } catch (error) {
        update((state) => ({ ...state, error: errorMessage(error) }));
        throw error;
      }
    },

    scheduleUpdate(note: Note, title: string, content: string) {
      pendingSaves.set(note.uuid, { note, title, content });
      if (saveTimer) clearTimeout(saveTimer);
      update((state) => ({ ...state, saving: true, error: null }));
      saveTimer = setTimeout(() => {
        saveTimer = null;
        void persistPendingSave().catch(() => undefined);
      }, autoSaveDelayMs);
    },

    flushPending: persistPendingSave,

    cancelPending() {
      if (saveTimer) clearTimeout(saveTimer);
      saveTimer = null;
      pendingSaves.clear();
      update((state) => ({ ...state, saving: saveInFlight !== null }));
    },

    async setPinned(note: Note, pinned: boolean) {
      await persistPendingSave();
      try {
        const updatedNote = await api.setPinned(note.uuid, pinned);
        replaceNote(updatedNote);
        markChanged();
        return updatedNote;
      } catch (error) {
        update((state) => ({ ...state, error: errorMessage(error) }));
        throw error;
      }
    },

    async setColor(note: Note, color: NoteColor) {
      await persistPendingSave();
      try {
        const updatedNote = await api.setColor(note.uuid, color);
        replaceNote(updatedNote);
        markChanged();
        return updatedNote;
      } catch (error) {
        update((state) => ({ ...state, error: errorMessage(error) }));
        throw error;
      }
    },

    async remove(note: Note) {
      try {
        const deletedNote = await api.delete(note.uuid);
        update((state) => ({
          ...state,
          items: state.items.filter((item) => item.uuid !== note.uuid),
          error: null,
        }));
        markChanged();
        return deletedNote;
      } catch (error) {
        update((state) => ({ ...state, error: errorMessage(error) }));
        throw error;
      }
    },

    async restore(note: Note) {
      try {
        const restoredNote = await api.restore(note.uuid);
        update((state) => ({
          ...state,
          items: [...state.items, restoredNote].sort(sortNotes),
          error: null,
        }));
        markChanged();
        return restoredNote;
      } catch (error) {
        update((state) => ({ ...state, error: errorMessage(error) }));
        throw error;
      }
    },

    setSearchQuery(searchQuery: string) {
      update((state) => ({ ...state, searchQuery }));
    },

    markSynced() {
      update((state) => ({ ...state, dirty: false, dirtySince: null }));
    },
  };
}

function sortNotes(left: Note, right: Note) {
  return (
    Number(right.pinned) - Number(left.pinned) ||
    right.updated_at - left.updated_at ||
    left.uuid.localeCompare(right.uuid)
  );
}

function errorMessage(error: unknown) {
  return localizedErrorMessage(error);
}

export const notes = createNoteStore();
export const visibleNotes = derived(notes, ($notes) => {
  const query = $notes.searchQuery.trim().toLocaleLowerCase();
  if (!query) return $notes.items;
  return $notes.items.filter(
    (note) =>
      note.title.toLocaleLowerCase().includes(query) ||
      note.content.toLocaleLowerCase().includes(query),
  );
});
