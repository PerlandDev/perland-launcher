import { useSyncExternalStore } from 'react';

// Whether this player has opened Personalize yet: until they have, the brush in the title bar carries a small "new"
// lamp. A per-viewer convenience kept in localStorage; every read and write may throw (blocked or cleared storage).

const KEY = 'perland.personalizeSeen';

/** Opened in this session, so the lamp goes out even where storage can't keep it. */
let openedNow = false;
const listeners = new Set<() => void>();

/** Seen, or unknowable: when storage can't be read the launcher can't promise the lamp will ever go out, so it never
 *  lights it. */
function snapshot(): boolean {
  if (openedNow) return true;
  try {
    return localStorage.getItem(KEY) !== null;
  } catch {
    return true;
  }
}

/** Personalize was opened, by whatever path: the brush is no longer new. */
export function markPersonalizeSeen() {
  if (openedNow) return;
  openedNow = true;
  try {
    localStorage.setItem(KEY, '1');
  } catch {
    // ignore: storage is optional, and the session flag already holds it
  }
  listeners.forEach((listener) => listener());
}

export function usePersonalizeSeen(): boolean {
  return useSyncExternalStore((listener) => {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, snapshot);
}

/** Tests: forget the session flag and, unless `keepStored` (a fresh launch), the stored one too. */
export function resetPersonalizeSeen({ keepStored = false }: { keepStored?: boolean } = {}) {
  openedNow = false;
  if (!keepStored) {
    try {
      localStorage.removeItem(KEY);
    } catch {
      // ignore
    }
  }
  listeners.forEach((listener) => listener());
}
