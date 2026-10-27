import { useSyncExternalStore } from 'react';

/** The one sign-in prompt: opened when Play needs a Microsoft account that owns Minecraft, from anywhere (a refused launch,
 *  an early check before a join), and drawn once by the app. */
let open = false;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((listener) => listener());

export function openSignInPrompt() {
  if (open) return;
  open = true;
  notify();
}

export function closeSignInPrompt() {
  if (!open) return;
  open = false;
  notify();
}

export function signInPromptSnapshot(): boolean {
  return open;
}

export function useSignInPrompt(): boolean {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    signInPromptSnapshot,
  );
}
