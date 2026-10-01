import { get, writable } from 'svelte/store';
export interface ProgressPanelTarget { uuid: string; title: string }
export const taskProgressPanel = writable<ProgressPanelTarget | null>(null);
export function openTaskProgress(uuid: string, title: string) {
  if (!get(taskProgressPanel)) taskProgressPanel.set({ uuid, title });
}
