import { useSyncExternalStore } from 'react';

export interface Toast {
  id: number;
  error: unknown;
}

let toasts: Toast[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((listener) => listener());

export function pushError(error: unknown) {
  toasts = [...toasts, { id: nextId++, error }];
  notify();
}

export function dismissToast(id: number) {
  toasts = toasts.filter((toast) => toast.id !== id);
  notify();
}

export function clearToasts() {
  toasts = [];
  notify();
}

export function toastsSnapshot(): Toast[] {
  return toasts;
}

export function useToasts(): Toast[] {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    toastsSnapshot,
  );
}
