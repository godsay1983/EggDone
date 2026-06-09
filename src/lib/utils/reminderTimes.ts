export type ReminderChoice = "none" | "same-day-9" | "previous-day-9" | "custom";

const TEN_MINUTES = 10 * 60 * 1000;
const TWO_HOURS = 2 * 60 * 60 * 1000;
const LATER_TODAY_HOUR = 18;

export function inferReminderChoice(
  dueDate: string | null,
  reminderAt: number | null,
): ReminderChoice {
  if (!dueDate || reminderAt === null) return "none";
  const sameDay = localReminderTime(dueDate, 0);
  const previousDay = localReminderTime(dueDate, -1);
  if (reminderAt === sameDay) return "same-day-9";
  if (reminderAt === previousDay) return "previous-day-9";
  return "custom";
}

export function reminderAtForDate(
  date: string,
  choice: ReminderChoice,
  customDateTime: string,
) {
  if (choice === "same-day-9") return localReminderTime(date, 0);
  if (choice === "previous-day-9") return localReminderTime(date, -1);
  if (choice === "custom") return dateTimeLocalToTimestamp(customDateTime);
  return null;
}

export function localReminderTime(date: string, offsetDays: number) {
  const [year, month, day] = date.split("-").map(Number);
  const reminderDate = new Date(year, month - 1, day, 9, 0, 0, 0);
  reminderDate.setDate(reminderDate.getDate() + offsetDays);
  return reminderDate.getTime();
}

export function defaultCustomReminderDateTime(date: string) {
  return `${date}T09:00`;
}

export function timestampToDateTimeLocal(timestamp: number) {
  const date = new Date(timestamp);
  const year = date.getFullYear();
  const month = padDatePart(date.getMonth() + 1);
  const day = padDatePart(date.getDate());
  const hour = padDatePart(date.getHours());
  const minute = padDatePart(date.getMinutes());
  return `${year}-${month}-${day}T${hour}:${minute}`;
}

export function dateTimeLocalToTimestamp(value: string) {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(value);
  if (!match) return null;
  const [, year, month, day, hour, minute] = match;
  const timestamp = new Date(
    Number(year),
    Number(month) - 1,
    Number(day),
    Number(hour),
    Number(minute),
    0,
    0,
  ).getTime();
  return Number.isFinite(timestamp) && timestamp >= 0 ? timestamp : null;
}

export function snoozeReminderAt(now = new Date()) {
  return now.getTime() + TEN_MINUTES;
}

export function laterTodayReminderAt(now = new Date()) {
  const laterToday = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate(),
    LATER_TODAY_HOUR,
    0,
    0,
    0,
  );

  if (laterToday.getTime() > now.getTime()) {
    return laterToday.getTime();
  }

  return now.getTime() + TWO_HOURS;
}

function padDatePart(value: number) {
  return value.toString().padStart(2, "0");
}
