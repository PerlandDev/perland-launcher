// Browse remembers its last kind and each kind's filters for the session (in memory only).
import type { ContentKind, LoaderKind, SortIndex } from '../../ipc/schema';

export interface Filters {
  query: string;
  /** null: Downloads for an empty query, Best match otherwise. */
  sort: SortIndex | null;
  minecraft: string | null;
  loader: LoaderKind | null;
  categories: string[];
}

const EMPTY: Filters = { query: '', sort: null, minecraft: null, loader: null, categories: [] };
let lastKind: ContentKind = 'modpack';
const byKind = new Map<string, Filters>();

export const sessionKind = (): ContentKind => lastKind;

export function sessionFilters(kind: ContentKind, scoped: boolean): Filters {
  return byKind.get(`${kind}:${scoped}`) ?? EMPTY;
}

export function remember(kind: ContentKind, scoped: boolean, filters: Filters) {
  if (!scoped) lastKind = kind;
  byKind.set(`${kind}:${scoped}`, filters);
}

export function resetBrowseSession() {
  lastKind = 'modpack';
  byKind.clear();
}
