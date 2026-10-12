// Progress of Browse installs, per instance. Kept apart from runStore so an install never
// shows on Play.
import { useSyncExternalStore } from 'react';
import type { AppEvent } from '../ipc/schema';
import type { ProgressView } from './runStore';

let progress = new Map<string, ProgressView>();
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((listener) => listener());

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
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    () => (id ? contentProgressSnapshot(id) : null),
  );
}

export function resetContentProgress() {
  progress = new Map();
  notify();
}
