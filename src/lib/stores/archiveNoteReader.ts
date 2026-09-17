import { taskNoteLinkApi } from "$lib/api/taskNoteLinkApi";
import { contentSearchApi } from "$lib/api/contentSearchApi";
import type { TaskNoteLinkView } from "$lib/types/taskNoteLink";

export function createArchiveNoteReader(links = taskNoteLinkApi, search = contentSearchApi) {
  async function list(uuid: string): Promise<TaskNoteLinkView[]> {
    return (await links.list("todo", uuid)).filter(item => item.link.deleted_at === null && item.note_state === "active");
  }
  return {
    list,
    async open(uuid: string, linkUuid: string) {
      const item = (await list(uuid)).find(item => item.link.uuid === linkUuid);
      if (!item) throw new Error("LINK_UNAVAILABLE");
      return search.resolve("note", item.link.note_uuid);
    },
  };
}
