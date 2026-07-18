import { describe, expect, it } from "vitest";

import { parseQuickAdd } from "./quickAdd";
import contract from "../../../docs/fixtures/quick-add-i18n-v1.json";

interface ContractExpected {
  title: string;
  due: { kind: "date" | "datetime"; local_date: string; local_time: string | null } | null;
  reminder: { local_date: string; local_time: string } | null;
  repeat_rule: "daily" | "weekdays" | "weekly" | "monthly" | null;
  group_uuid: string | null;
  priority: 0 | 1;
  parse_applied: boolean;
}

interface ContractCase {
  id: string;
  input: string;
  expected: ContractExpected;
}

describe("parseQuickAdd", () => {
  const now = new Date("2026-06-10T12:00:00+08:00");

  it("extracts today and tomorrow as date-only schedules", () => {
    expect(parseQuickAdd("今天 写日报", now)).toEqual({
      title: "写日报",
      schedule: {
        due_date: "2026-06-10",
        due_at: null,
        reminder_at: null,
        repeat_rule: null,
      },
      label: "今天",
      groupName: null,
      priority: 0,
    });

    expect(parseQuickAdd("买鸡蛋 明天", now).schedule?.due_date).toBe(
      "2026-06-11",
    );
  });

  it("extracts weekday words as the next matching local date", () => {
    expect(parseQuickAdd("周五 发周报", now)).toMatchObject({
      title: "发周报",
      schedule: { due_date: "2026-06-12" },
      label: "周五",
    });

    expect(parseQuickAdd("下周五 复盘", now)).toMatchObject({
      title: "复盘",
      schedule: { due_date: "2026-06-19" },
      label: "下周五",
    });
  });

  it("extracts explicit time when a date token is present", () => {
    const result = parseQuickAdd("明天 10:30 开会", now);

    expect(result.title).toBe("开会");
    expect(result.schedule?.due_date).toBeNull();
    expect(result.schedule?.due_at).toBe(
      new Date(2026, 5, 11, 10, 30, 0, 0).getTime(),
    );
    expect(result.label).toBe("明天 10:30");
  });

  it("keeps the original title when parsing would leave it empty", () => {
    expect(parseQuickAdd("明天 10:30", now)).toEqual({
      title: "明天 10:30",
      schedule: null,
      label: "",
      groupName: null,
      priority: 0,
    });
  });

  it("ignores unsupported phrases without blocking creation", () => {
    expect(parseQuickAdd("月底整理票据", now)).toEqual({
      title: "月底整理票据",
      schedule: null,
      label: "",
      groupName: null,
      priority: 0,
    });
  });

  it("extracts known group tags without creating unknown groups", () => {
    expect(parseQuickAdd("#工作 写方案", now, ["工作", "生活"])).toEqual({
      title: "写方案",
      schedule: null,
      label: "",
      groupName: "工作",
      priority: 0,
    });

    expect(parseQuickAdd("#工作 明天 10:00 写方案", now, ["工作"])).toMatchObject({
      title: "写方案",
      label: "明天 10:00",
      groupName: "工作",
    });

    expect(parseQuickAdd("#不存在 写方案", now, ["工作"])).toEqual({
      title: "#不存在 写方案",
      schedule: null,
      label: "",
      groupName: null,
      priority: 0,
    });
  });

  it("extracts leading priority marker", () => {
    expect(parseQuickAdd("! 明天 写方案", now)).toMatchObject({
      title: "写方案",
      schedule: { due_date: "2026-06-11" },
      label: "明天",
      priority: 1,
    });

    expect(parseQuickAdd("！ #工作 写方案", now, ["工作"])).toEqual({
      title: "写方案",
      schedule: null,
      label: "",
      groupName: "工作",
      priority: 1,
    });
  });

  it.each(contract.cases as ContractCase[])("matches shared i18n contract: $id", (fixture) => {
    const referenceNow = new Date(contract.reference.now_local);
    const groupNames = contract.reference.groups.map((group) => group.name);
    const result = parseQuickAdd(fixture.input, referenceNow, groupNames);
    const expected = fixture.expected;
    const expectedGroupName = contract.reference.groups.find(
      (group) => group.uuid === expected.group_uuid,
    )?.name ?? null;

    expect(result.title).toBe(expected.title);
    expect(result.groupName).toBe(expectedGroupName);
    expect(result.priority).toBe(expected.priority);
    expect(result.schedule?.repeat_rule ?? null).toBe(expected.repeat_rule);
    expect(Boolean(result.schedule || result.groupName || result.priority || result.title !== fixture.input))
      .toBe(expected.parse_applied);
    assertContractDue(result.schedule, expected.due);
    assertContractReminder(result.schedule?.reminder_at ?? null, expected.reminder);
  });
});

function assertContractDue(
  schedule: ReturnType<typeof parseQuickAdd>["schedule"],
  due: ContractExpected["due"],
) {
  if (due === null) {
    expect(schedule?.due_date ?? null).toBeNull();
    expect(schedule?.due_at ?? null).toBeNull();
    return;
  }
  if (due.kind === "date") {
    expect(schedule?.due_date).toBe(due.local_date);
    expect(schedule?.due_at).toBeNull();
    return;
  }
  expect(schedule?.due_date).toBeNull();
  expect(schedule?.due_at).toBe(localTimestamp(due.local_date, due.local_time ?? "00:00"));
}

function assertContractReminder(
  reminderAt: number | null,
  reminder: ContractExpected["reminder"],
) {
  expect(reminderAt).toBe(
    reminder === null ? null : localTimestamp(reminder.local_date, reminder.local_time),
  );
}

function localTimestamp(date: string, time: string) {
  const [year, month, day] = date.split("-").map(Number);
  const [hour, minute] = time.split(":").map(Number);
  return new Date(year, month - 1, day, hour, minute, 0, 0).getTime();
}
