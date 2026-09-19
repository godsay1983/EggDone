import { beforeEach, expect, it, vi } from 'vitest';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
import { dailyPlanApi } from './dailyPlanApi';

beforeEach(() => { invoke.mockReset(); });

it('uses the fixed identity-only planning command contract', async () => {
  const request = { operation_uuid: 'operation', task_uuid: 'task', plan_date: '2026-09-19',
    action: 'up' as const, expected: 'revision' };
  await dailyPlanApi.list(request.plan_date);
  await dailyPlanApi.write(request);
  expect(invoke.mock.calls).toEqual([
    ['list_daily_plans', { date: request.plan_date }], ['write_daily_plan', { request }],
  ]);
});

it('preserves backend failures for conflict and retry handling', async () => {
  invoke.mockRejectedValue('PLAN_CONFLICT');
  await expect(dailyPlanApi.list('2026-09-19')).rejects.toBe('PLAN_CONFLICT');
});
