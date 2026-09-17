import { expect, it } from 'vitest';
import { archiveDateHasPassed } from './archiveDate';
it('uses the local day for all-day tasks and timestamp for timed tasks', () => {
  const now = new Date(2026,8,17,12);
  expect(archiveDateHasPassed({due_at:null,due_date:'2026-09-16'},now)).toBe(true);
  expect(archiveDateHasPassed({due_at:null,due_date:'2026-09-17'},now)).toBe(false);
  expect(archiveDateHasPassed({due_at:null,due_date:null},now)).toBe(false);
  expect(archiveDateHasPassed({due_at:now.getTime()-1,due_date:'2026-09-17'},now)).toBe(true);
  expect(archiveDateHasPassed({due_at:now.getTime()+1,due_date:'2026-09-17'},now)).toBe(false);
});
