const units = ['KB', 'MB', 'GB'] as const;

export function formatBytes(bytes: number, lang: string): string {
  let value = bytes / 1000;
  let unit: (typeof units)[number] = 'KB';
  for (const next of units.slice(1)) {
    if (value < 1000) break;
    value /= 1000;
    unit = next;
  }
  const digits = value < 10 && unit !== 'KB' ? 1 : 0;
  const shown = new Intl.NumberFormat(lang, { maximumFractionDigits: digits }).format(Math.max(value, 1));
  return `${shown} ${unit}`;
}

export function formatCount(n: number, lang: string): string {
  return new Intl.NumberFormat(lang).format(n);
}

/** Memory in GB for labels: 6144 → "6", 2560 → "2.5". */
export function formatGb(mb: number, lang: string): string {
  return new Intl.NumberFormat(lang, { maximumFractionDigits: 1 }).format(mb / 1024);
}

/** 81000000 → "81M" (es: "81 M"), for download and follow counts. */
export function formatCompact(n: number, lang: string): string {
  return new Intl.NumberFormat(lang, { notation: 'compact', maximumFractionDigits: 1 }).format(n);
}

/** Seconds left as a clock: 600 → "10:00", 65 → "1:05". */
export function formatCountdown(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}
