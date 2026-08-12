import { describe, expect, it } from "vitest";
import { formatHistoryTime } from "./historyFormatting";

describe("formatHistoryTime", () => {
  it("uses local calendar days and fixed zero padding", () => {
    const now = new Date(2026, 7, 13, 12, 0);
    expect(formatHistoryTime(new Date(2026, 7, 13, 8, 5).getTime(), now)).toBe(
      "今天 08:05",
    );
    expect(formatHistoryTime(new Date(2026, 7, 12, 23, 7).getTime(), now)).toBe(
      "昨天 23:07",
    );
    expect(formatHistoryTime(new Date(2026, 6, 2, 3, 4).getTime(), now)).toBe(
      "2026-07-02 03:04",
    );
  });

  it("handles invalid timestamps", () => {
    expect(formatHistoryTime(Number.NaN)).toBe("时间未知");
  });
});
