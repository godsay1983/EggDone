import { derived, writable } from "svelte/store";

import { noteApi } from "$lib/api/noteApi";
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
  onChanged: () => void = () => undefined,
  autoSaveDelayMs = AUTO_SAVE_DELAY_MS,
) {
  const store = writable<NoteState>({ ...initialState });
  const { subscribe, update } = store;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingSave: PendingNoteSave | null = null;

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
    const pending = pendingSave;
    pendingSave = null;
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (!pending) {
      update((state) => ({ ...state, saving: false }));
      return null;
    }

    try {
      const note = await api.update(
        pending.note.uuid,
        pending.title,
        pending.content,
      );
      replaceNote(note);
      markChanged();
      return note;
    } catch (error) {
      update((state) => ({ ...state, error: errorMessage(error) }));
      throw error;
    } finally {
      update((state) => ({ ...state, saving: pendingSave !== null }));
    }
  }

  return {
    subscribe,

    async load() {
      update((state) => ({ ...state, loading: true, error: null }));
      try {
        const items = await api.list();
        update((state) => ({
          ...state,
          items: [...items].sort(sortNotes),
          loading: false,
          error: null,
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
          error: null,
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
      pendingSave = { note, title, content };
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
      pendingSave = null;
      update((state) => ({ ...state, saving: false }));
    },

    async setPinned(note: Note, pinned: boolean) {
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
  return error instanceof Error ? error.message : String(error);
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
