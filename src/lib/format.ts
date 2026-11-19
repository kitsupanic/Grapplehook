/** `1.5 MB`; units are 1024 apart. `?` when the value is unknown. */
export function formatBytes(bytes: number | null | undefined) {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return '?';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(value < 10 && unit > 0 ? 1 : 0)} ${units[unit]}`;
}

/** h:mm:ss or m:ss. */
export function formatDuration(seconds: number) {
  const s = Math.round(seconds);
  const mm = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, '0');
  const h = Math.floor(s / 3600);
  return h > 0 ? `${h}:${String(mm).padStart(2, '0')}:${ss}` : `${mm}:${ss}`;
}

/** Claim numbers on the tags: `007`. */
export const claimNo = (n: number) => String(n).padStart(3, '0');
