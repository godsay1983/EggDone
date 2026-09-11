import { invoke } from '@tauri-apps/api/core';

export interface GeneralPreferences {
  version: number;
  revision: number;
  values: Record<string, string | null>;
}

export const readGeneralPreferences = () => invoke<GeneralPreferences | null>('get_general_preferences');
export const migrateGeneralPreferences = (values: GeneralPreferences['values']) =>
  invoke<GeneralPreferences>('initialize_general_preferences', { values });
export const patchGeneralPreference = (key: string, value: string | null) =>
  invoke<GeneralPreferences>('patch_general_preference', { key, value });
