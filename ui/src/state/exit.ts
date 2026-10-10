import type { ExitInfo } from '../ipc/schema';

/** Same rule as Rust's `ExitInfo::is_crash`: an exit nobody asked for that wasn't clean. */
export function isCrash(exit: ExitInfo): boolean {
  return !exit.killed && !exit.cancelled && exit.error === null && exit.code !== 0;
}

/** 2 GB more, never above three quarters of this computer's memory. Null when there's no room. */
export function suggestMemory(currentMb: number, systemMb: number): number | null {
  const ceiling = Math.floor((systemMb * 0.75) / 1024) * 1024;
  const next = Math.min(currentMb + 2048, ceiling, 65536);
  return next > currentMb ? next : null;
}
