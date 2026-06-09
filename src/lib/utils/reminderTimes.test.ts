import { describe, expect, it } from "vitest";

import {
  dateTimeLocalToTimestamp,
  defaultCustomReminderDateTime,
  inferReminderChoice,
  laterTodayReminderAt,
  reminderAtForDate,
  snoozeReminderAt,
  timestampToDateTimeLocal,
} from "./reminderTimes";

describe("reminder times", () => {
  it("infers preset and custom reminder choices", () => {
    expect(
      inferReminderChoice(
        "2026-06-09",
        reminderAtForDate("2026-06-09", "same-day-9", ""),
      ),
    ).toBe("same-day-9");
    expect(
      inferReminderChoice(
        "2026-06-09",
        reminderAtForDate("2026-06-09", "previous-day-9", ""),
      ),
    ).toBe("previous-day-9");
    expect(
      inferReminderChoice(
        "2026-06-09",
        reminderAtForDate("2026-06-09", "custom", "2026-06-09T14:30"),
      ),
    ).toBe("custom");
    expect(inferReminderChoice("2026-06-09", null)).toBe("none");
  });

  it("round trips local datetime input values", () => {
    const timestamp = dateTimeLocalToTimestamp("2026-06-09T14:30");

    expect(timestamp).not.toBeNull();
    expect(timestampToDateTimeLocal(timestamp!)).toBe("2026-06-09T14:30");
    expect(defaultCustomReminderDateTime("2026-06-09")).toBe("2026-06-09T09:00");
    expect(dateTimeLocalToTimestamp("2026/06/09 14:30")).toBeNull();
  });

  it("snoozes reminders by ten minutes", () => {
    const now = new Date(2026, 5, 9, 10, 0, 0, 0);

    expect(snoozeReminderAt(now)).toBe(new Date(2026, 5, 9, 10, 10, 0, 0).getTime());
  });

  it("uses 18:00 for later today before evening and two hours otherwise", () => {
    const afternoon = new Date(2026, 5, 9, 14, 30, 0, 0);
    const evening = new Date(2026, 5, 9, 19, 30, 0, 0);

    expect(laterTodayReminderAt(afternoon)).toBe(
      new Date(2026, 5, 9, 18, 0, 0, 0).getTime(),
    );
    expect(laterTodayReminderAt(evening)).toBe(
      new Date(2026, 5, 9, 21, 30, 0, 0).getTime(),
    );
  });
});
