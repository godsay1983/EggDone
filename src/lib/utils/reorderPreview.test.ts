import { describe, expect, it } from "vitest";

import { movePreviewByPointer } from "./reorderPreview";

describe("movePreviewByPointer", () => {
  it("moves downward one row at a time", () => {
    const centers = [
      { id: 1, centerY: 20 },
      { id: 2, centerY: 60 },
      { id: 3, centerY: 100 },
    ];

    const second = movePreviewByPointer([1, 2, 3], 1, 70, centers);
    expect(second).toEqual([2, 1, 3]);

    const third = movePreviewByPointer(second, 1, 110, centers);
    expect(third).toEqual([2, 3, 1]);
  });

  it("returns upward through the middle row before the first row", () => {
    const order = [2, 3, 1];
    const centers = [
      { id: 2, centerY: 20 },
      { id: 3, centerY: 60 },
      { id: 1, centerY: 100 },
    ];

    const second = movePreviewByPointer(order, 1, 50, centers);
    expect(second).toEqual([2, 1, 3]);

    const first = movePreviewByPointer(second, 1, 10, centers);
    expect(first).toEqual([1, 2, 3]);
  });

  it("never skips an adjacent row in one pointer update", () => {
    const order = [2, 3, 1];
    const centers = [
      { id: 2, centerY: 20 },
      { id: 3, centerY: 60 },
      { id: 1, centerY: 100 },
    ];

    expect(movePreviewByPointer(order, 1, 0, centers)).toEqual([2, 1, 3]);
  });
});
