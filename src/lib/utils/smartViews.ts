export type SmartViewId = 'overdue' | 'next7' | 'no_date' | 'important' | 'recently_completed';

export const SMART_VIEW_IDS: SmartViewId[] = [
  'overdue', 'next7', 'no_date', 'important', 'recently_completed'
];

export function normalizeSmartView(value: string | null): SmartViewId | null {
  return value === 'overdue' || value === 'next7' || value === 'no_date' ||
    value === 'important' || value === 'recently_completed' ? value : null;
}

export interface SmartViewTask {
  completed: boolean;
  completedAt: number | null;
  deletedAt: number | null;
  archivedAt: number | null;
  dueAt: number | null;
  dueDate: string | null;
  priority: number;
}

export function matchesSmartView(task: SmartViewTask, view: SmartViewId, now: number): boolean {
  if (task.deletedAt !== null || task.archivedAt !== null || !Number.isFinite(now)) return false;
  const current = new Date(now);
  const today = new Date(current.getFullYear(), current.getMonth(), current.getDate()).getTime();
  // Calendar constructors preserve local-day boundaries through DST transitions.
  if (view === 'recently_completed') {
    const start = new Date(current.getFullYear(), current.getMonth(), current.getDate() - 6).getTime();
    return task.completed && task.completedAt !== null &&
      task.completedAt >= start && task.completedAt <= now;
  }
  if (task.completed) return false;
  if (view === 'important') return task.priority === 1;
  if (view === 'no_date') return task.dueAt === null && !task.dueDate;
  let due = task.dueAt;
  if (task.dueDate) {
    const parts = task.dueDate.split('-');
    if (parts.length !== 3) return false;
    const year = Number(parts[0]);
    const month = Number(parts[1]);
    const day = Number(parts[2]);
    const date = new Date(year, month - 1, day);
    if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) return false;
    due = date.getTime();
  }
  if (due === null || !Number.isFinite(due)) return false;
  if (view === 'overdue') return due < today;
  const end = new Date(current.getFullYear(), current.getMonth(), current.getDate() + 7).getTime();
  return due >= today && due < end;
}
