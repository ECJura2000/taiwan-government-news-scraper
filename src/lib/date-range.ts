export function previousCompleteWeek(now = new Date()): { start: string; end: string } {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Taipei", year: "numeric", month: "2-digit", day: "2-digit",
  }).formatToParts(now);
  const part = (name: string) => Number(parts.find(item => item.type === name)?.value);
  const today = new Date(Date.UTC(part("year"), part("month") - 1, part("day")));
  const daysFromMonday = (today.getUTCDay() + 6) % 7;
  const end = new Date(today);
  end.setUTCDate(today.getUTCDate() - daysFromMonday - 1);
  const start = new Date(end);
  start.setUTCDate(end.getUTCDate() - 6);
  return { start: start.toISOString().slice(0, 10), end: end.toISOString().slice(0, 10) };
}

export function rangeError(start: string, end: string): string {
  if (!start || !end) return "請填寫起始與結束日期。";
  if (start > end) return "起始日期不可晚於結束日期。";
  return "";
}
