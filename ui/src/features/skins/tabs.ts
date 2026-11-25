/** The studio's tabs: build from pieces, paint by hand, or pick a look from Perland's catalog. */
export const TABS = ['create', 'paint', 'catalog'] as const;
export type Tab = (typeof TABS)[number];
/** Where the studio opens. */
export const FIRST_TAB: Tab = 'paint';
