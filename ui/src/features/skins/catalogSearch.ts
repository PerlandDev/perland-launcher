// Searches the catalog by name in the current language, or by tag (its id or its name in the current language), ignoring
// case and accents.
import type { CatalogLook, CatalogManifest, CatalogPiece } from '../../ipc/schema';

export type Lang = 'en' | 'es';

const fold = (s: string) => s.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase().trim();

function matches(manifest: CatalogManifest, lang: Lang, name: string, tags: readonly string[], query: string): boolean {
  const q = fold(query);
  if (q === '' || fold(name).includes(q)) return true;
  const label = new Map(manifest.tags.map((t) => [t.id, t.name[lang]]));
  return tags.some((t) => fold(t).includes(q) || fold(label.get(t) ?? '').includes(q));
}

/** `tag` narrows to looks carrying that tag id; '' keeps all. Retired looks are hidden. */
export function searchLooks(manifest: CatalogManifest, query: string, tag: string, lang: Lang): CatalogLook[] {
  return manifest.looks.filter((l) => !l.retired && (tag === '' || l.tags.includes(tag)) && matches(manifest, lang, l.name[lang], l.tags, query));
}

/** Retired pieces show only when their id is in `worn`. */
export function searchPieces(manifest: CatalogManifest, slot: string, query: string, lang: Lang, worn?: ReadonlySet<string>): CatalogPiece[] {
  return manifest.pieces.filter((p) => p.slot === slot && (!p.retired || worn?.has(p.id)) && matches(manifest, lang, p.name[lang], p.tags, query));
}
