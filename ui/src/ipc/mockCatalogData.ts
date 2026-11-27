// The files of the mock skin catalog: the Engine's generated catalog, copied from PerlandEngine's
// internal/skincatalog/data (CC0, Perland's own art) by
//   node ui/scripts/sync-mock-catalog.mjs <engine>/internal/skincatalog/data
// The tests read a small fixed catalog in its place (src/test/catalogFixtureData.ts, through the alias in
// vite.config.ts), so they never load the hundreds of files here and do not change when the catalog grows.
import manifest from './mock-catalog/manifest.json';

/** The PNGs the manifest names, inlined as data URLs, by path. */
export const pngs = import.meta.glob<string>('./mock-catalog/*.png', { query: '?inline', import: 'default', eager: true });

export { manifest };
