import { useSyncExternalStore } from 'react';

interface ActivityState {
  open: boolean;
  focus: string | null;
}

let state: ActivityState = { open: false, focus: null };
const listeners = new Set<() => void>();
const set = (next: ActivityState) => {
  state = next;
  listeners.forEach((listener) => listener());
};

/** Open the activity drawer, optionally on one instance's tab (used when a game crashes). */
export function openActivity(focus: string | null = state.focus) {
  set({ open: true, focus });
}

export function closeActivity() {
  set({ ...state, open: false });
}

export function resetActivity() {
  set({ open: false, focus: null });
}

export function useActivity(): ActivityState {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    () => state,
  );
}
