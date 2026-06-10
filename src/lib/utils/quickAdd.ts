export interface QuickAddSchedule {
  due_date: string | null;
  due_at: number | null;
  reminder_at: null;
  repeat_rule: null;
}

export interface QuickAddResult {
  title: string;
  schedule: QuickAddSchedule | null;
  label: string;
  groupName: string | null;
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
];

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
  if (!dateMatch && !groupMatch) {
    return emptyResult(originalTitle);
  }

  const timeMatch = findTimeToken(originalTitle);
  const tokens = [
    ...(dateMatch ? [dateMatch.token] : []),
    ...(timeMatch ? [timeMatch.token] : []),
    ...(groupMatch ? [groupMatch.token] : []),
  ];
  const title = removeTokens(originalTitle, tokens);
  if (!title) {
    return emptyResult(originalTitle);
  }

  if (!dateMatch) {
    return {
      title,
      schedule: null,
      label: "",
      groupName: groupMatch?.name ?? null,
    };
  }

  if (timeMatch) {
    const dueAt = localDateTimeToTimestamp(
      dateMatch.date,
      timeMatch.hour,
      timeMatch.minute,
    );
    if (dueAt === null) return emptyResult(originalTitle);
    return {
      title,
      schedule: {
        due_date: null,
        due_at: dueAt,
        reminder_at: null,
        repeat_rule: null,
      },
      label: `${dateMatch.label} ${timeMatch.text}`,
      groupName: groupMatch?.name ?? null,
    };
  }

  return {
    title,
    schedule: {
      due_date: dateMatch.date,
      due_at: null,
      reminder_at: null,
      repeat_rule: null,
    },
    label: dateMatch.label,
    groupName: groupMatch?.name ?? null,
  };
}

function emptyResult(title: string): QuickAddResult {
  return {
    title,
    schedule: null,
    label: "",
    groupName: null,
  };
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
  if (!weekdayMatch || weekdayMatch.index === undefined) return null;
  const targetWeekday = WEEKDAY_MAP.get(weekdayMatch[2]);
  if (targetWeekday === undefined) return null;
  return {
    date: localDateString(
      daysUntilWeekday(now.getDay(), targetWeekday, weekdayMatch[1] === "下"),
      now,
    ),
    label: weekdayMatch[0],
    token: tokenFromMatch(weekdayMatch),
  };
}

function findTimeToken(input: string) {
  const match = /(?:^|[\s，,])([01]?\d|2[0-3])[:：]([0-5]\d)(?=$|[\s，,])/.exec(
    input,
  );
  if (!match || match.index === undefined) return null;
  const leadingSeparator = match[0].length - `${match[1]}:${match[2]}`.length;
  const start = match.index + Math.max(0, leadingSeparator);
  const text = `${match[1].padStart(2, "0")}:${match[2]}`;
  return {
    hour: Number(match[1]),
    minute: Number(match[2]),
    text,
    token: {
      start,
      end: start + match[1].length + 1 + match[2].length,
    },
  };
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
