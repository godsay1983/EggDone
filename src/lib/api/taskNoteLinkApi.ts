import { invoke } from "@tauri-apps/api/core";
import type { LinkedTodoDraft, LinkScope, TaskNoteLink, TaskNoteLinkView } from "$lib/types/taskNoteLink";

export const taskNoteLinkApi = {
  list: (scope: LinkScope, uuid: string) => invoke<TaskNoteLinkView[]>("list_task_note_links", { scope, uuid }),
  pair: (todoUuid: string, noteUuid: string) => invoke<TaskNoteLink | null>("get_task_note_link", { todoUuid, noteUuid }),
  create: (draft: LinkedTodoDraft) => invoke<TaskNoteLink>("create_linked_todo", { draft }),
  change: (todoUuid: string, noteUuid: string, active: boolean, expected: TaskNoteLink | null) =>
    invoke<TaskNoteLink | null>("change_task_note_link", { todoUuid, noteUuid, active, expected }),
};
