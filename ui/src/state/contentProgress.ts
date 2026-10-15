// Progress of Browse installs, per instance. Kept apart from runStore so an install never
// shows on Play.
import { useSyncExternalStore } from 'react';
import type { AppEvent } from '../ipc/schema';
import type { ProgressView } from './runStore';

let progress = new Map<string, ProgressView>();
/** Browse installs in flight (from the click until browse_install answers). Restart now waits for them. */
let installs = 0;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((listener) => listener());
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};

export function applyContentProgress(event: Extract<AppEvent, { type: 'progress' }>) {
  if (!event.instance) return;
  const { task, done, total, bytes } = event;
  progress = new Map(progress).set(event.instance, { task, done, total, bytes });
  notify();
}

export function clearContentProgress(id: string) {
  if (!progress.has(id)) return;
  const next = new Map(progress);
  next.delete(id);
  progress = next;
  notify();
}

export const contentProgressSnapshot = (id: string): ProgressView | null => progress.get(id) ?? null;

export function useContentProgress(id: string | null): ProgressView | null {
  return useSyncExternalStore(subscribe, () => (id ? contentProgressSnapshot(id) : null));
}

/** A Browse install starts; call the returned function (once) when it ends, however it ends. */
export function beginContentInstall(): () => void {
  installs += 1;
  notify();
  let ended = false;
  return () => {
    if (ended) return;
    ended = true;
    installs -= 1;
    notify();
  };
}

export const contentInstalling = () => installs > 0;

export function useContentInstalling(): boolean {
  return useSyncExternalStore(subscribe, contentInstalling);
}

export function resetContentProgress() {
  progress = new Map();
  installs = 0;
  notify();
}
