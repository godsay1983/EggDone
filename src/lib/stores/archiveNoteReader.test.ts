import { afterEach, expect, it, vi } from 'vitest';
import { taskNoteLinkApi } from '$lib/api/taskNoteLinkApi';
import { contentSearchApi } from '$lib/api/contentSearchApi';
import type { TaskNoteLinkView } from '$lib/types/taskNoteLink';
import { createArchiveNoteReader } from './archiveNoteReader';
afterEach(() => vi.restoreAllMocks());
function link(state: TaskNoteLinkView['note_state'] = 'active', deleted: number | null = null): TaskNoteLinkView {
  return {link:{uuid:'link',todo_uuid:'task',note_uuid:'note',created_at:1,updated_at:1,updated_by:'test',deleted_at:deleted},
    todo_title:'Task',note_title:'Note',todo_state:'archived',note_state:state,is_repeating:false};
}
it('shows only effective links and revalidates before reading current note text', async () => {
  const list = vi.spyOn(taskNoteLinkApi,'list').mockResolvedValue([link(),link('deleted'),link('missing'),link('active',2)]);
  const resolve = vi.spyOn(contentSearchApi,'resolve').mockResolvedValue({kind:'note',uuid:'note',title:'Current title',content:'Current text',parent_uuid:null,parent_title:null,completed:false,archived:false});
  const reader = createArchiveNoteReader(); expect(await reader.list('task')).toEqual([link()]);
  expect((await reader.open('task','link')).content).toBe('Current text'); expect(resolve).toHaveBeenCalledWith('note','note');
  list.mockResolvedValue([]); await expect(reader.open('task','link')).rejects.toThrow('LINK_UNAVAILABLE');
  expect(resolve).toHaveBeenCalledTimes(1);
});
it('keeps read failures visible instead of claiming no related notes', async () => {
  vi.spyOn(taskNoteLinkApi,'list').mockRejectedValue(Error('read failed'));
  await expect(createArchiveNoteReader().list('task')).rejects.toThrow('read failed');
});
