// Progress of the running import from another launcher. It belongs to no instance, so it is
// kept apart from runStore and never shows on Play.
import { useSyncExternalStore } from 'react';
import type { AppEvent } from '../ipc/schema';
import type { ProgressView } from './runStore';

let progress: ProgressView | null = null;
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

/** The import ended (however it ended): its progress goes. */
export function clearImportProgress() {
  if (progress === null) return;
  progress = null;
  notify();
}

export const importProgressSnapshot = (): ProgressView | null => progress;

export function useImportProgress(): ProgressView | null {
  return useSyncExternalStore(subscribe, importProgressSnapshot);
}
