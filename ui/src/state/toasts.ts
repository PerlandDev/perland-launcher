import { useSyncExternalStore } from 'react';
import { isLauncherError } from '../ipc/errors';
import { openSignInPrompt } from './signInPrompt';

export interface Notice {
  title: string;
  body?: string;
  action?: { label: string; run: () => void };
}

export interface Toast {
  id: number;
  error?: unknown;
  notice?: Notice;
}

let toasts: Toast[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((listener) => listener());

/** The refusals that mean "sign in to play": the sign-in prompt says that, with the way to do it, so no toast as well. */
const SIGN_IN_CODES: ReadonlySet<string> = new Set(['no_account', 'microsoft_sign_in_again']);

export function pushError(error: unknown) {
  if (isLauncherError(error) && SIGN_IN_CODES.has(error.code)) {
    openSignInPrompt();
    return;
  }
  toasts = [...toasts, { id: nextId++, error }];
  notify();
}

export function pushNotice(notice: Notice) {
  toasts = [...toasts, { id: nextId++, notice }];
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
