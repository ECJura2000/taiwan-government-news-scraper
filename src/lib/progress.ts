import type { ProgressEvent } from "./contracts";

const SOURCE_PHASE_MAX_PERCENT = 90;

export function calculateRunPercent(
  progress: ProgressEvent | null,
  completed: number,
  total: number,
): number {
  if (!progress) return 0;
  if (progress.kind === "completed") return 100;
  if (progress.kind === "writing_report") return 99;
  if (progress.kind === "writing_outputs") return 97;
  if (progress.kind === "ranking") return 95;
  if (progress.kind === "processing_items") return 92;
  if (total <= 0) return 0;

  const safeCompleted = Math.min(Math.max(completed, 0), total);
  return Math.round((safeCompleted / total) * SOURCE_PHASE_MAX_PERCENT);
}

export function advanceCompleted(current: number, event: ProgressEvent): number {
  return event.completed === undefined ? current : Math.max(current, event.completed);
}

export function advanceRunPercent(
  current: number,
  event: ProgressEvent,
  completed: number,
  total: number,
): number {
  return Math.max(current, calculateRunPercent(event, completed, total));
}
