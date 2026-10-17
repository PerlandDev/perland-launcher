// The Servers screen remembers its sort and filters for the session (in memory only), like Browse.
import type { ListQuery } from '../../ipc/schema';

export type ListFilters = Omit<ListQuery, 'offset'>;

export const NO_FILTERS: ListFilters = {
  sort: 'players',
  modded: null,
  minecraft: null,
  hasRoom: false,
  category: null,
  language: null,
  region: null,
};

let remembered: ListFilters = NO_FILTERS;

export const sessionServerFilters = (): ListFilters => remembered;

export function rememberServerFilters(filters: ListFilters) {
  remembered = filters;
}

/** True when any filter (not the sort) narrows the list. */
export const isFiltered = (f: ListFilters) =>
  f.modded !== null || f.minecraft !== null || f.hasRoom || f.category !== null || f.language !== null || f.region !== null;

export function resetServersSession() {
  remembered = NO_FILTERS;
}
