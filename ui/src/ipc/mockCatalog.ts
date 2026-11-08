// The mock skin catalog for `pnpm --filter ui dev:mock` and the tests: the Engine's generated first batch, copied from
// PerlandEngine's internal/skincatalog/data (CC0, Perland's own art). The PNGs are inlined as data URLs, as
// `skin_catalog_file` answers. Only the mock imports this module, and the mock is loaded in mock mode only, so none of it
// reaches the production bundle.
import manifestJson from './mock-catalog/manifest.json';
import { catalogManifest, type CatalogManifest } from './schema';

const PNGS = import.meta.glob<string>('./mock-catalog/*.png', { query: '?inline', import: 'default', eager: true });

export interface MockCatalog {
  manifest: CatalogManifest;
  /** Every file the manifest names, as a data URL, by hash. */
  files: Map<string, string>;
}

export function mockCatalog(): MockCatalog {
  const files = new Map<string, string>();
  for (const [path, url] of Object.entries(PNGS)) {
    const hash = /([0-9a-f]{64})\.png$/.exec(path)?.[1];
    if (hash) files.set(hash, url);
  }
  return { manifest: catalogManifest.parse(manifestJson), files };
}
