// Human-friendly formatting for durations, dates and sizes.

export function duration(secs: number): string {
  if (secs < 60) return `${secs} sec`;
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (h === 0) return `${m} min`;
  return m === 0 ? `${h} hr` : `${h} hr ${m} min`;
}

const time = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const day = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" });

/** "today at 18:00", "yesterday", "3 days ago", "Oct 20, 2026". */
export function relative(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  const startOf = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((startOf(today) - startOf(d)) / 86_400_000);
  if (days <= 0) return `today at ${time.format(d)}`;
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  return day.format(d);
}

export function date(iso: string): string {
  return day.format(new Date(iso));
}

export function memory(mb: number): string {
  return mb >= 1024 ? `${(mb / 1024).toFixed(mb % 1024 === 0 ? 0 : 1)} GB` : `${mb} MB`;
}
