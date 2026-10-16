// The running import from another launcher: what it copies and how far it got. It belongs to no
// instance, so it is kept apart from runStore and never shows on Play. It lives here, not in the
// form, because the import keeps going when the player leaves the form: coming back shows it again.
import { useSyncExternalStore } from 'react';
import type { AppEvent } from '../ipc/schema';
import type { ProgressView } from './runStore';

/** What the form shows of an import it didn't start (the player left and came back). */
export interface RunningImport {
  path: string;
  name: string;
  sizeBytes: number;
  /** The player pressed Cancel; the copy stops before its next file. */
  cancelling: boolean;
}

let progress: ProgressView | null = null;
let running: RunningImport | null = null;
/** The last import's failure, while a form is there to show it. */
let failure: unknown = null;
/** How many import forms are on screen: none means the player is elsewhere. */
let watchers = 0;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((listener) => listener());
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};

export function applyImportProgress(event: Extract<AppEvent, { type: 'progress' }>) {
  const { task, done, total, bytes } = event;
  progress = { task, done, total, bytes };
  notify();
}

/** An import starts: until it ends, every import form shows its copy. */
export function beginImport(started: Omit<RunningImport, 'cancelling'>) {
  running = { ...started, cancelling: false };
  progress = null;
  failure = null;
  notify();
}

/** The player asked to stop it. False when there is nothing to stop or it was asked already. */
export function markImportCancelling(): boolean {
  if (!running || running.cancelling) return false;
  running = { ...running, cancelling: true };
  notify();
  return true;
}

/** The import ended. `error` is kept for the form to show, when one is on screen. */
export function endImport(error: unknown = null) {
  running = null;
  progress = null;
  failure = watchers > 0 ? error : null;
  notify();
}

/** An import form is on screen until the returned function runs. */
export function watchImport(): () => void {
  watchers += 1;
  return () => {
    watchers = Math.max(0, watchers - 1);
    // A failure shown in a form that's gone isn't news the next time.
    if (watchers === 0 && failure !== null) {
      failure = null;
      notify();
    }
  };
}

export const importWatched = () => watchers > 0;

/** Forget everything (tests). */
export function resetImport() {
  progress = null;
  running = null;
  failure = null;
  watchers = 0;
  notify();
}

export const importProgressSnapshot = (): ProgressView | null => progress;
export const importRunningSnapshot = (): RunningImport | null => running;
const failureSnapshot = (): unknown => failure;

export function useImportProgress(): ProgressView | null {
  return useSyncExternalStore(subscribe, importProgressSnapshot);
}

export function useRunningImport(): RunningImport | null {
  return useSyncExternalStore(subscribe, importRunningSnapshot);
}

export function useImportFailure(): unknown {
  return useSyncExternalStore(subscribe, failureSnapshot);
}
