export function archiveDateHasPassed(item: { due_at: number | null; due_date: string | null }, now = new Date()): boolean {
  if (item.due_at !== null) return item.due_at < now.getTime();
  const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
  return item.due_date !== null && item.due_date < today;
}
