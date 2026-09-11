import { describe, expect, it } from "vitest";
import type { ProgressEvent } from "./contracts";
import { advanceCompleted, advanceRunPercent, calculateRunPercent } from "./progress";

function event(kind: ProgressEvent["kind"], completed?: number): ProgressEvent {
  return { kind, completed, total: 72 };
}

describe("run progress", () => {
  it("uses completed sources for the 0–90 percent collection phase", () => {
    expect(calculateRunPercent(event("started", 0), 0, 72)).toBe(0);
    expect(calculateRunPercent(event("source_started"), 0, 72)).toBe(0);
    expect(calculateRunPercent(event("source_finished", 1), 1, 72)).toBe(1);
    expect(calculateRunPercent(event("source_finished", 8), 8, 72)).toBe(10);
    expect(calculateRunPercent(event("source_finished", 36), 36, 72)).toBe(45);
    expect(calculateRunPercent(event("source_finished", 72), 72, 72)).toBe(90);
  });

  it("advances through each post-processing stage", () => {
    expect(calculateRunPercent(event("processing_items", 72), 72, 72)).toBe(92);
    expect(calculateRunPercent(event("ranking", 72), 72, 72)).toBe(95);
    expect(calculateRunPercent(event("writing_outputs", 72), 72, 72)).toBe(97);
    expect(calculateRunPercent(event("writing_report", 72), 72, 72)).toBe(99);
    expect(calculateRunPercent(event("completed"), 72, 72)).toBe(100);
  });

  it("never lets a late event reduce the completed count", () => {
    expect(advanceCompleted(8, event("source_started"))).toBe(8);
    expect(advanceCompleted(8, event("source_finished", 7))).toBe(8);
    expect(advanceCompleted(8, event("source_finished", 9))).toBe(9);
  });

  it("does not move backward when a late failure follows an output stage", () => {
    expect(advanceRunPercent(97, event("failed", 72), 72, 72)).toBe(97);
  });
});
