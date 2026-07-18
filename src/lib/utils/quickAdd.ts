import type { RepeatRule } from "$lib/types";

export interface QuickAddSchedule {
  due_date: string | null;
  due_at: number | null;
  reminder_at: number | null;
  repeat_rule: RepeatRule | null;
}

export interface QuickAddResult {
  title: string;
  schedule: QuickAddSchedule | null;
  label: string;
  groupName: string | null;
  priority: 0 | 1;
}

interface MatchedToken {
  start: number;
  end: number;
}

const DATE_TOKEN_PATTERNS = [
  { pattern: /大后天/, offsetDays: 3, label: "大后天" },
  { pattern: /后天/, offsetDays: 2, label: "后天" },
  { pattern: /明天/, offsetDays: 1, label: "明天" },
  { pattern: /今天/, offsetDays: 0, label: "今天" },
  { pattern: /\bday after tomorrow\b/i, offsetDays: 2, label: "day after tomorrow" },
  { pattern: /\btomorrow\b/i, offsetDays: 1, label: "tomorrow" },
  { pattern: /\btoday\b/i, offsetDays: 0, label: "today" },
];

const ENGLISH_WEEKDAY_MAP = new Map([
  ["sunday", 0],
  ["monday", 1],
  ["tuesday", 2],
  ["wednesday", 3],
  ["thursday", 4],
  ["friday", 5],
  ["saturday", 6],
]);

const WEEKDAY_MAP = new Map([
  ["一", 1],
  ["二", 2],
  ["三", 3],
  ["四", 4],
  ["五", 5],
  ["六", 6],
  ["日", 0],
  ["天", 0],
]);

export function parseQuickAdd(
  input: string,
  now = new Date(),
  groupNames: string[] = [],
): QuickAddResult {
  const originalTitle = input.trim();
  if (!originalTitle) {
    return emptyResult(originalTitle);
  }

  const dateMatch = findDateToken(originalTitle, now);
  const groupMatch = findGroupToken(originalTitle, groupNames);
  const priorityMatch = findPriorityToken(originalTitle);
  const reminderMatch = findReminderToken(originalTitle);
  const repeatMatch = findRepeatToken(originalTitle);
  if (!dateMatch && !groupMatch && !priorityMatch && !reminderMatch && !repeatMatch) {
    return emptyResult(originalTitle);
  }

  const timeMatch = findTimeToken(originalTitle, reminderMatch?.token ?? null);
  const tokens = [
    ...(priorityMatch ? [priorityMatch] : []),
    ...(dateMatch ? [dateMatch.token] : []),
    ...(timeMatch ? [timeMatch.token] : []),
    ...(reminderMatch ? [reminderMatch.token] : []),
    ...(repeatMatch ? [repeatMatch.token] : []),
    ...(groupMatch ? [groupMatch.token] : []),
  ];
  const title = removeTokens(originalTitle, tokens);
  if (!title) {
    return emptyResult(originalTitle);
  }

  if (!dateMatch && !reminderMatch && !repeatMatch) {
    return {
      title,
      schedule: null,
      label: "",
      groupName: groupMatch?.name ?? null,
      priority: priorityMatch ? 1 : 0,
    };
  }

  const scheduleDate = dateMatch?.date ?? localDateString(0, now);
  const reminderAt = reminderMatch
    ? localDateTimeToTimestamp(scheduleDate, reminderMatch.hour, reminderMatch.minute)
    : null;

  if (timeMatch) {
    const dueAt = localDateTimeToTimestamp(
      scheduleDate,
      timeMatch.hour,
      timeMatch.minute,
    );
    if (dueAt === null) return emptyResult(originalTitle);
    return {
      title,
      schedule: {
        due_date: null,
        due_at: dueAt,
        reminder_at: reminderAt,
        repeat_rule: repeatMatch?.rule ?? null,
      },
      label: [dateMatch?.label, timeMatch.text, repeatMatch?.label].filter(Boolean).join(" "),
      groupName: groupMatch?.name ?? null,
      priority: priorityMatch ? 1 : 0,
    };
  }

  return {
    title,
    schedule: {
      due_date: scheduleDate,
      due_at: null,
      reminder_at: reminderAt,
      repeat_rule: repeatMatch?.rule ?? null,
    },
    label: [dateMatch?.label, repeatMatch?.label].filter(Boolean).join(" "),
    groupName: groupMatch?.name ?? null,
    priority: priorityMatch ? 1 : 0,
  };
}

function emptyResult(title: string): QuickAddResult {
  return {
    title,
    schedule: null,
    label: "",
    groupName: null,
    priority: 0,
  };
}

function findPriorityToken(input: string): MatchedToken | null {
  const match = /^(?:[!！]|important\b)(?=$|[\s，,])/i.exec(input);
  if (!match || match.index === undefined) return null;
  return tokenFromMatch(match);
}

function findDateToken(input: string, now: Date) {
  for (const candidate of DATE_TOKEN_PATTERNS) {
    const match = candidate.pattern.exec(input);
    if (!match || match.index === undefined) continue;
    return {
      date: localDateString(candidate.offsetDays, now),
      label: candidate.label,
      token: tokenFromMatch(match),
    };
  }

  const weekdayMatch = /(下)?(?:周|星期)([一二三四五六日天])/.exec(input);
  if (weekdayMatch && weekdayMatch.index !== undefined) {
    const targetWeekday = WEEKDAY_MAP.get(weekdayMatch[2]);
    if (targetWeekday !== undefined) {
      return {
        date: localDateString(
          daysUntilWeekday(now.getDay(), targetWeekday, weekdayMatch[1] === "下"),
          now,
        ),
        label: weekdayMatch[0],
        token: tokenFromMatch(weekdayMatch),
      };
    }
  }

  const englishWeekday = /\b(next\s+)?(sunday|monday|tuesday|wednesday|thursday|friday|saturday)\b/i.exec(input);
  if (!englishWeekday || englishWeekday.index === undefined) return null;
  const targetWeekday = ENGLISH_WEEKDAY_MAP.get(englishWeekday[2].toLowerCase());
  if (targetWeekday === undefined) return null;
  return {
    date: localDateString(daysUntilWeekday(now.getDay(), targetWeekday, Boolean(englishWeekday[1])), now),
    label: englishWeekday[0],
    token: tokenFromMatch(englishWeekday),
  };
}

function findTimeToken(input: string, excluded: MatchedToken | null) {
  const pattern = /(?:^|[\s，,])(?:at\s+)?(\d{1,2})(?:[:：]([0-5]\d))?\s*(am|pm)?(?=$|[\s，,])/gi;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(input)) !== null) {
    if (match.index === undefined) continue;
    if (!/\bat\s/i.test(match[0]) && !/[:：]/.test(match[0]) && !/(?:am|pm)/i.test(match[0])) {
      continue;
    }
    const leadingSeparator = /^\s|^，|^,/.test(match[0]) ? 1 : 0;
    const start = match.index + leadingSeparator;
    const end = match.index + match[0].length;
    if (excluded && start < excluded.end && end > excluded.start) continue;
    const parsed = parseClock(match[1], match[2], match[3]);
    if (!parsed) continue;
    return {
      ...parsed,
      text: `${String(parsed.hour).padStart(2, "0")}:${String(parsed.minute).padStart(2, "0")}`,
      token: { start, end },
    };
  }
  return null;
}

function findReminderToken(input: string) {
  const match = /(?:提醒|remind(?:\s+me)?)(?:\s*(?:在|at))?\s*(\d{1,2})(?:[:：]([0-5]\d))?\s*(am|pm)?\b/i.exec(input);
  if (!match || match.index === undefined) return null;
  const parsed = parseClock(match[1], match[2], match[3]);
  if (!parsed) return null;
  return {
    ...parsed,
    token: tokenFromMatch(match),
  };
}

function findRepeatToken(input: string): { rule: RepeatRule; label: string; token: MatchedToken } | null {
  const candidates: Array<{ pattern: RegExp; rule: RepeatRule }> = [
    { pattern: /(?:每天|\bevery day\b|\bdaily\b)/i, rule: "daily" },
    { pattern: /(?:工作日|\bweekdays\b)/i, rule: "weekdays" },
    { pattern: /(?:每周|\bevery week\b|\bweekly\b)/i, rule: "weekly" },
    { pattern: /(?:每月|\bevery month\b|\bmonthly\b)/i, rule: "monthly" },
  ];
  for (const candidate of candidates) {
    const match = candidate.pattern.exec(input);
    if (!match || match.index === undefined) continue;
    return { rule: candidate.rule, label: match[0], token: tokenFromMatch(match) };
  }
  return null;
}

function parseClock(hourText: string, minuteText?: string, meridiemText?: string) {
  let hour = Number(hourText);
  const minute = Number(minuteText ?? "0");
  const meridiem = meridiemText?.toLowerCase();
  if (meridiem) {
    if (hour < 1 || hour > 12) return null;
    if (meridiem === "am") hour = hour === 12 ? 0 : hour;
    if (meridiem === "pm") hour = hour === 12 ? 12 : hour + 12;
  }
  if (hour < 0 || hour > 23 || minute < 0 || minute > 59) return null;
  return { hour, minute };
}

function findGroupToken(input: string, groupNames: string[]) {
  if (groupNames.length === 0) return null;
  const knownGroups = new Set(groupNames.map((name) => name.trim()));
  const matches = input.matchAll(/(?:^|[\s，,])#([^\s#，,]+)/g);
  for (const match of matches) {
    if (match.index === undefined) continue;
    const name = match[1].trim();
    if (!knownGroups.has(name)) continue;
    const markerIndex = match[0].indexOf("#");
    const start = match.index + markerIndex;
    return {
      name,
      token: {
        start,
        end: start + name.length + 1,
      },
    };
  }
  return null;
}

function daysUntilWeekday(
  currentWeekday: number,
  targetWeekday: number,
  nextWeek: boolean,
) {
  let offset = (targetWeekday - currentWeekday + 7) % 7;
  if (nextWeek) offset += offset === 0 ? 7 : 7;
  return offset;
}

function localDateString(offsetDays: number, baseDate: Date) {
  const date = new Date(baseDate);
  date.setDate(date.getDate() + offsetDays);
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function localDateTimeToTimestamp(date: string, hour: number, minute: number) {
  const [year, month, day] = date.split("-").map(Number);
  const timestamp = new Date(year, month - 1, day, hour, minute, 0, 0).getTime();
  return Number.isFinite(timestamp) && timestamp >= 0 ? timestamp : null;
}

function tokenFromMatch(match: RegExpExecArray): MatchedToken {
  return {
    start: match.index,
    end: match.index + match[0].length,
  };
}

function removeTokens(input: string, tokens: MatchedToken[]) {
  const sorted = [...tokens].sort((left, right) => right.start - left.start);
  let title = input;
  for (const token of sorted) {
    title = `${title.slice(0, token.start)} ${title.slice(token.end)}`;
  }
  return title.replace(/\s+/g, " ").trim();
}
