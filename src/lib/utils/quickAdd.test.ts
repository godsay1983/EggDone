import { describe, expect, it } from "vitest";

import { parseQuickAdd } from "./quickAdd";

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
    });
  });

  it("ignores unsupported phrases without blocking creation", () => {
    expect(parseQuickAdd("月底整理票据", now)).toEqual({
      title: "月底整理票据",
      schedule: null,
      label: "",
      groupName: null,
    });
  });

  it("extracts known group tags without creating unknown groups", () => {
    expect(parseQuickAdd("#工作 写方案", now, ["工作", "生活"])).toEqual({
      title: "写方案",
      schedule: null,
      label: "",
      groupName: "工作",
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
    });
  });
});
