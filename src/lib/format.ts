import type { ScanProgress } from "./types";

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function progressLabel(p: ScanProgress | null): string {
  if (!p) return "Scanning…";
  return `Scanning… ${p.done} of ${p.total} checked (${p.title})`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(1)} ${UNITS[unit]}`;
}
