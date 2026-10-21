import { useSyncExternalStore } from 'react';

/** The most lines kept per server; older ones fall off the top. */
export const MANAGE_LOG_MAX = 1000;

export interface ManageLog {
  lines: string[];
  /** The live log stopped on its own (`server_log_ended`); the console offers Reconnect. */
  ended: boolean;
}

const EMPTY: ManageLog = { lines: [], ended: false };
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
    set(tenantId, stack, (log) => ({ lines: log.lines.concat(lines).slice(-MANAGE_LOG_MAX), ended: false }));
  },
  end(tenantId: string, stack: string) {
    set(tenantId, stack, (log) => ({ ...log, ended: true }));
  },
  /** The stream is (re)opened: it is live again. The lines stay, so a reconnect continues them. */
  resume(tenantId: string, stack: string) {
    if (!(logs.get(keyOf(tenantId, stack))?.ended ?? false)) return;
    set(tenantId, stack, (log) => ({ ...log, ended: false }));
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
