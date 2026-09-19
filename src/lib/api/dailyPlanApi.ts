import { invoke } from '@tauri-apps/api/core';

export interface DailyPlanEntry {
  task_uuid: string;
  plan_date: string;
  status: 'planned' | 'completed';
  position: number;
}

export interface DailyPlanSnapshot {
  date: string;
  revision: string;
  current: DailyPlanEntry[];
  previous: DailyPlanEntry[];
}

export type DailyPlanAction = 'add' | 'remove' | 'up' | 'down';

export interface DailyPlanRequest {
  operation_uuid: string;
  task_uuid: string;
  plan_date: string;
  action: DailyPlanAction;
  expected: string;
}

export const dailyPlanApi = {
  list: (date: string) => invoke<DailyPlanSnapshot>('list_daily_plans', { date }),
  write: (request: DailyPlanRequest) => invoke<DailyPlanSnapshot>('write_daily_plan', { request }),
};
