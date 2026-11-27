// Searches the catalog by name in the current language, or by tag (its id or its name in the current language), ignoring
// case and accents.
import type { CatalogLook, CatalogManifest, CatalogPiece, SkinModel } from '../../ipc/schema';

export type Lang = 'en' | 'es';

const fold = (s: string) => s.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase().trim();

type Labels = ReadonlyMap<string, string>;
const tagLabels = (manifest: CatalogManifest, lang: Lang): Labels => new Map(manifest.tags.map((t) => [t.id, t.name[lang]]));

/** `q` is already folded. */
function matches(labels: Labels, name: string, tags: readonly string[], q: string): boolean {
  if (q === '' || fold(name).includes(q)) return true;
  return tags.some((t) => fold(t).includes(q) || fold(labels.get(t) ?? '').includes(q));
}

/** `tag` narrows to looks carrying that tag id; '' keeps all. Retired looks are hidden. */
export function searchLooks(manifest: CatalogManifest, query: string, tag: string, lang: Lang): CatalogLook[] {
  const labels = tagLabels(manifest, lang);
  const q = fold(query);
  return manifest.looks.filter((l) => !l.retired && (tag === '' || l.tags.includes(tag)) && matches(labels, l.name[lang], l.tags, q));
}

/** How the Catalog tab orders its looks: as the catalog lists them, or by name. */
export type LookSort = 'featured' | 'name';
/** What the Catalog tab is showing: a search, one tag ('' for all), one arm model ('' for both) and an order. */
export interface LookFilter {
  query: string;
  tag: string;
  arms: '' | SkinModel;
  sort: LookSort;
}

export const NO_FILTER: LookFilter = { query: '', tag: '', arms: '', sort: 'featured' };

/** How many looks the Catalog tab's grid draws at first, and how many more each time its end comes near. */
export const GRID_PAGE = 60;

/** Nothing searched or filtered: the whole catalog is showing (in whatever order). */
export const noFilter = (filter: LookFilter) => filter.query.trim() === '' && filter.tag === '' && filter.arms === '';

/** The looks a filter shows, in its order. */
export function browseLooks(manifest: CatalogManifest, filter: LookFilter, lang: Lang): CatalogLook[] {
  const looks = searchLooks(manifest, filter.query, filter.tag, lang).filter((l) => filter.arms === '' || l.model === filter.arms);
  if (filter.sort !== 'name') return looks;
  const collator = new Intl.Collator(lang, { sensitivity: 'base', numeric: true });
  return looks.sort((a, b) => collator.compare(a.name[lang], b.name[lang]));
}

export interface TagFacet {
  id: string;
  name: string;
  count: number;
}

/**
 * The tag keys for a filter: the tags that still have a look under its search and arms, in the catalog's order, each
 * with how many (what choosing it would show), and `total`, what All shows. The chosen tag keeps its key even at 0, so
 * it can be seen and let go.
 */
export function tagFacets(manifest: CatalogManifest, filter: LookFilter, lang: Lang): { total: number; tags: TagFacet[] } {
  const looks = browseLooks(manifest, { ...filter, tag: '', sort: 'featured' }, lang);
  const counts = new Map<string, number>();
  for (const look of looks) for (const tag of new Set(look.tags)) counts.set(tag, (counts.get(tag) ?? 0) + 1);
  const tags = manifest.tags.map((t) => ({ id: t.id, name: t.name[lang], count: counts.get(t.id) ?? 0 })).filter((t) => t.count > 0 || t.id === filter.tag);
  return { total: looks.length, tags };
}

export interface LookShelf extends TagFacet {
  /** The first `size` looks carrying the tag. */
  looks: CatalogLook[];
}

/** How many looks a shelf shows. */
export const SHELF_SIZE = 12;

/** One shelf per tag that has a look among `looks`, in the catalog's order with New first; `looks` keep their order. */
export function lookShelves(manifest: CatalogManifest, looks: readonly CatalogLook[], lang: Lang, size = SHELF_SIZE): LookShelf[] {
  const byTag = new Map<string, CatalogLook[]>();
  for (const look of looks) {
    for (const tag of new Set(look.tags)) {
      const shelf = byTag.get(tag);
      if (shelf) shelf.push(look);
      else byTag.set(tag, [look]);
    }
  }
  const tags = [...manifest.tags.filter((t) => t.id === 'new'), ...manifest.tags.filter((t) => t.id !== 'new')];
  return tags.flatMap((t) => {
    const shelf = byTag.get(t.id);
    return shelf ? [{ id: t.id, name: t.name[lang], count: shelf.length, looks: shelf.slice(0, size) }] : [];
  });
}

/** Retired pieces show only when their id is in `worn`. */
export function searchPieces(manifest: CatalogManifest, slot: string, query: string, lang: Lang, worn?: ReadonlySet<string>): CatalogPiece[] {
  const labels = tagLabels(manifest, lang);
  const q = fold(query);
  return manifest.pieces.filter((p) => p.slot === slot && (!p.retired || worn?.has(p.id)) && matches(labels, p.name[lang], p.tags, q));
}
