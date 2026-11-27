// The skin catalog the tests read in place of the mock's (see the alias in vite.config.ts): the Engine's first batch
// of 22 pieces and 3 looks, with thumbnail sheets and an empty expressions list. It stays as it is while the real
// catalog grows; a test that needs expressions or hundreds of looks builds them on top (skinFakes.ts, manyLooks.ts).
import manifest from './catalog-fixture/manifest.json';

/** The PNGs the manifest names, inlined as data URLs, by path. */
export const pngs = import.meta.glob<string>('./catalog-fixture/*.png', { query: '?inline', import: 'default', eager: true });

export { manifest };
