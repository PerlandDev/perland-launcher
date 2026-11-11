// A catalog as big as the real one is growing to be, for tests: the mock's looks cloned under new ids, names and tags.
import type { CatalogLook, CatalogManifest } from '../ipc/schema';

const FIRST = ['Amber', 'Brave', 'Calm', 'Dusty', 'Eager', 'Frosty', 'Golden', 'Happy', 'Icy', 'Jolly'] as const;
const PRIMERO = ['Ámbar', 'Valiente', 'Tranquilo', 'Polvoriento', 'Ansioso', 'Helado', 'Dorado', 'Feliz', 'Gélido', 'Alegre'] as const;

/** How many looks `manyLooks` marks New: the first ones, as a catalog update would. */
export const NEW_LOOKS = 20;

/**
 * `count` looks made from the manifest's own (so their pieces, arms and thumbnails are real), in place of them.
 * Look `i` is named "<word> look <i>" ("Amber look 000", "Brave look 001", …: the word cycles through ten), wears arms
 * that alternate classic and slim, and carries one tag, the manifest's tags in turn (`new` skipped), plus `new` on the
 * first `NEW_LOOKS`. The sheets are kept as they are: only the file's hash matters to the tests.
 */
export function manyLooks(manifest: CatalogManifest, count: number): CatalogManifest {
  const tags = manifest.tags.map((t) => t.id).filter((id) => id !== 'new');
  const looks: CatalogLook[] = Array.from({ length: count }, (_, i) => {
    const from = manifest.looks[i % manifest.looks.length]!;
    const n = String(i).padStart(3, '0');
    return {
      ...from,
      id: `look-many-${n}`,
      name: { en: `${FIRST[i % FIRST.length]} look ${n}`, es: `Estilo ${PRIMERO[i % PRIMERO.length]!.toLowerCase()} ${n}` },
      tags: [...(i < NEW_LOOKS ? ['new'] : []), tags[i % tags.length]!],
      model: i % 2 === 0 ? 'classic' : 'slim',
      // Each its own thumbnail file, as the Engine's looks have: none of them is in the mock, only the sheet is.
      thumb: i.toString(16).padStart(64, '0'),
      retired: false,
    };
  });
  return { ...manifest, version: `many-${count}`, looks };
}
