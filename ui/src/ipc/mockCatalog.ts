// The mock skin catalog for `pnpm --filter ui dev:mock`: the Engine's generated catalog (see mockCatalogData.ts). The
// PNGs are inlined as data URLs, as `skin_catalog_file` answers. Only the mock imports this module, and the mock is
// loaded in mock mode only, so none of it reaches the production bundle. The tests get a small fixed catalog instead.
import { manifest as manifestJson, pngs as PNGS } from './mockCatalogData';
import { catalogManifest, type CatalogManifest } from './schema';

export interface MockCatalog {
  manifest: CatalogManifest;
  /** Every file the manifest names (thumbnail sheets included), as a data URL, by hash. */
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
