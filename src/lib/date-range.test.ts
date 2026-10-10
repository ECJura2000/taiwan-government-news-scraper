import { describe, expect, it } from "vitest";
import { previousCompleteWeek, rangeError } from "./date-range";

describe("complete reporting week in Taiwan", () => {
  it("uses the same complete week on Monday, Saturday and Sunday", () => {
    for (const date of ["2026-10-05T02:00:00Z", "2026-10-10T02:00:00Z", "2026-10-11T02:00:00Z"]) {
      expect(previousCompleteWeek(new Date(date))).toEqual({ start: "2026-09-28", end: "2026-10-04" });
    }
  });
  it("uses Taiwan calendar dates across UTC midnight and year boundaries", () => {
    expect(previousCompleteWeek(new Date("2026-10-11T16:00:00Z"))).toEqual({ start: "2026-10-05", end: "2026-10-11" });
    expect(previousCompleteWeek(new Date("2026-01-01T00:00:00Z"))).toEqual({ start: "2025-12-22", end: "2025-12-28" });
  });
  it("rejects incomplete and reversed custom ranges", () => {
    expect(rangeError("", "2026-10-04")).not.toBe("");
    expect(rangeError("2026-10-10", "2026-10-04")).not.toBe("");
    expect(rangeError("2026-10-04", "2026-10-04")).toBe("");
  });
});
