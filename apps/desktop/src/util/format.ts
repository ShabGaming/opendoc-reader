/** Download and file sizes, in decimal units like the servers and the file manager show them. */
export function formatBytes(bytes: number): string {
  if (bytes >= 1e9) return `${round1(bytes / 1e9)} GB`;
  if (bytes >= 1e8) return `${Math.round(bytes / 1e6)} MB`;
  if (bytes >= 1e6) return `${round1(bytes / 1e6)} MB`;
  if (bytes >= 1e3) return `${Math.round(bytes / 1e3)} KB`;
  return `${Math.round(bytes)} B`;
}

/** Download progress such as "0.91 / 1.80 GB": both numbers share the unit of the total. */
export function formatProgressBytes(done: number, total: number): string {
  const [unit, divisor, digits] =
    total >= 1e9 ? ["GB", 1e9, 2] : total >= 1e6 ? ["MB", 1e6, 0] : ["KB", 1e3, 0];
  const fmt = (n: number) => (n / divisor).toFixed(digits);
  return `${fmt(done)} / ${fmt(total)} ${unit}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** Memory sizes are given in MB by the hardware probe. */
export function formatMemoryMB(mb: number): string {
  return `${Math.round(mb / 1024)} GB`;
}

/** Free disk space in MB, shown in decimal GB as the file manager does. */
export function formatDiskMB(mb: number): string {
  return `${Math.round(mb / 1000)} GB`;
}

/** "14 s", "2 min 10 s", "1 h 5 min". */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  if (total < 60) return `${total} s`;
  if (total < 3600) {
    const m = Math.floor(total / 60);
    const s = total % 60;
    return s === 0 ? `${m} min` : `${m} min ${s} s`;
  }
  const h = Math.floor(total / 3600);
  const m = Math.round((total % 3600) / 60);
  return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

export function formatDurationMs(ms: number): string {
  return formatDuration(ms / 1000);
}

/** "G:" for a Windows path, null for anything else. */
export function driveOf(path: string): string | null {
  const match = /^([A-Za-z]:)/.exec(path);
  return match ? match[1].toUpperCase() : null;
}

/** The file name without its extension. */
export function baseName(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

export function parentDir(path: string): string {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut > 0 ? path.slice(0, cut) : path;
}

function round1(n: number): string {
  return (Math.round(n * 10) / 10).toFixed(1);
}

/** Seconds per page: one decimal under ten seconds ("4.8 s"), whole seconds above. */
export function formatPageTime(seconds: number): string {
  return seconds < 10 ? `${seconds.toFixed(1)} s` : formatDuration(seconds);
}
