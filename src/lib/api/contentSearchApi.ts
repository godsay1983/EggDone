import { invoke } from '@tauri-apps/api/core';

export type SearchScope = 'todo' | 'note' | 'attachment';
export interface SearchItem {
  kind: SearchScope;
  uuid: string;
  title: string;
  excerpt: string;
  parent_uuid: string | null;
  parent_title: string | null;
  completed: boolean;
  archived: boolean;
  updated_at: number;
  matched_field: 'title' | 'body' | 'filename';
}
export interface SearchPage {
  query: string;
  scope: SearchScope;
  offset: number;
  limit: number;
  total: number;
  items: SearchItem[];
}
export interface SearchTarget {
  kind: SearchScope;
  uuid: string;
  title: string;
  content: string;
  parent_uuid: string | null;
  parent_title: string | null;
  completed: boolean;
  archived: boolean;
}
export const contentSearchApi = {
  search: (scope: SearchScope, query: string, offset = 0, limit = 20) =>
    invoke<SearchPage>('search_content', { scope, query, offset, limit }),
  resolve: (scope: SearchScope, uuid: string) => invoke<SearchTarget>('resolve_search_target', { scope, uuid }),
};
