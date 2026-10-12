import { useSyncExternalStore } from 'react';
import type { ContentKind } from '../ipc/schema';

/** The content tabs of the Mods & packs screen; no tab means Mods. */
export type ModsTab = Exclude<ContentKind, 'modpack'>;

export type Route =
  | { name: 'home'; id: string | null }
  | { name: 'mods'; id: string; tab?: ModsTab }
  | { name: 'instanceSettings'; id: string }
  | { name: 'newInstance'; link?: string }
  | { name: 'browse'; kind: ContentKind; instance?: string; project?: string }
  // An empty `invite` means "ask for a link". `relinkFor` is the server instance being re-linked.
  | { name: 'joinServer'; invite: string; relinkFor?: string }
  | { name: 'accounts' }
  | { name: 'settings' };

const HOME: Route = { name: 'home', id: null };
const LAST_KEY = 'perland.lastInstance';

let current: Route = HOME;
const listeners = new Set<() => void>();

export function navigate(route: Route) {
  current = route;
  const id = instanceOf(route);
  if (id) rememberInstance(id);
  listeners.forEach((listener) => listener());
}

export function routeSnapshot(): Route {
  return current;
}

export function useRoute(): Route {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    routeSnapshot,
  );
}

export function instanceOf(route: Route): string | null {
  return 'id' in route ? route.id : null;
}

/** Per-viewer convenience: reopen the last instance. Storage may be unavailable; that's fine. */
export function rememberInstance(id: string) {
  try {
    localStorage.setItem(LAST_KEY, id);
  } catch {
    // ignore: storage is optional
  }
}

export function lastInstance(): string | null {
  try {
    return localStorage.getItem(LAST_KEY);
  } catch {
    return null;
  }
}

export function resetRoute() {
  current = HOME;
  try {
    localStorage.removeItem(LAST_KEY);
  } catch {
    // ignore
  }
  listeners.forEach((listener) => listener());
}
