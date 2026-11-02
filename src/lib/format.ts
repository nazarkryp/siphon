const UNITS = ["B", "KB", "MB", "GB", "TB"];

function scale(bytes: number): { value: number; unit: number } {
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return { value, unit };
}

function formatValue(value: number, unit: number): string {
  if (unit === 0) return String(Math.round(value));
  // Large values read better without a decimal: "212 GB", not "212.4 GB".
  return value >= 100 ? value.toFixed(0) : value.toFixed(1);
}

export function formatBytes(bytes: number): string {
  const { value, unit } = scale(bytes);
  return `${formatValue(value, unit)} ${UNITS[unit]}`;
}

/** "3.9 of 6.3 GB": the done value uses the unit of the total. */
export function formatBytesOf(done: number, total: number): string {
  const { value, unit } = scale(total);
  const doneValue = done / 1024 ** unit;
  return `${formatValue(doneValue, unit)} of ${formatValue(value, unit)} ${UNITS[unit]}`;
}

export function formatSize(bytes: number | null, isEstimate: boolean): string {
  if (bytes == null) return "Unknown size";
  return `${isEstimate ? "≈ " : ""}${formatBytes(bytes)}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatEta(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

export function formatBitrate(bitsPerSecond: number): string {
  if (bitsPerSecond >= 1_000_000) return `${(bitsPerSecond / 1_000_000).toFixed(1)} Mbps`;
  return `${Math.round(bitsPerSecond / 1000)} kbps`;
}

export function formatDuration(seconds: number): string {
  const total = Math.round(seconds);
  if (total < 60) return `${total} s`;
  const minutes = Math.round(total / 60);
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m > 0 ? `${h} h ${m} min` : `${h} h`;
}

export function formatPercent(fraction: number): string {
  return `${Math.floor(Math.min(1, Math.max(0, fraction)) * 100)}%`;
}

export function formatRelative(timeMs: number, nowMs: number): string {
  const seconds = Math.max(0, Math.round((nowMs - timeMs) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return new Date(timeMs).toLocaleDateString();
}
