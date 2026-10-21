import { useSyncExternalStore } from 'react';

/** The most lines kept per server; older ones fall off the top. */
export const MANAGE_LOG_MAX = 1000;

export interface ManageLog {
  lines: string[];
  /** The running number of `lines[0]` since the store began: a line keeps its number (a stable key) as older ones fall off. */
  first: number;
  /** The live log stopped on its own (`server_log_ended`); the console offers Reconnect. */
  ended: boolean;
}

const EMPTY: ManageLog = { lines: [], first: 0, ended: false };
const logs = new Map<string, ManageLog>();
const listeners = new Set<() => void>();

const keyOf = (tenantId: string, stack: string) => `${tenantId}/${stack}`;
const notify = () => listeners.forEach((listener) => listener());

function set(tenantId: string, stack: string, next: (log: ManageLog) => ManageLog) {
  const key = keyOf(tenantId, stack);
  logs.set(key, next(logs.get(key) ?? EMPTY));
  notify();
}

/** The console lines of the servers followed in Manage: an external store fed by `server_log` events. */
export const manageLog = {
  push(tenantId: string, stack: string, lines: string[]) {
    if (lines.length === 0) return;
    set(tenantId, stack, (log) => {
      const all = log.lines.concat(lines);
      const drop = Math.max(0, all.length - MANAGE_LOG_MAX);
      return { lines: all.slice(drop), first: log.first + drop, ended: false };
    });
  },
  end(tenantId: string, stack: string) {
    set(tenantId, stack, (log) => ({ ...log, ended: true }));
  },
  /** The stream is about to be (re)opened: every open sends the last lines again, so they replace what is shown rather
   *  than repeat it. Line numbers keep counting, so no key is reused. */
  clear(tenantId: string, stack: string) {
    const log = logs.get(keyOf(tenantId, stack));
    if (!log) return;
    set(tenantId, stack, () => ({ lines: [], first: log.first + log.lines.length, ended: false }));
  },
  get(tenantId: string, stack: string): ManageLog {
    return logs.get(keyOf(tenantId, stack)) ?? EMPTY;
  },
  reset() {
    logs.clear();
    notify();
  },
};

export function useManageLog(tenantId: string, stack: string): ManageLog {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    () => manageLog.get(tenantId, stack),
  );
}
