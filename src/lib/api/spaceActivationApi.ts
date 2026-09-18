import { invoke } from '@tauri-apps/api/core';
export interface SpaceReport {
  state: 'idle' | 'prepared' | 'active';
  mode: string;
  confirmation: string | null;
  objectKey: string | null;
}
export const spaceActivationApi = (action: 'status' | 'prepare' | 'activate', expected: string | null = null) =>
  invoke<SpaceReport>('migration_space', { action, expected });
