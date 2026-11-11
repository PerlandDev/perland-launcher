// The catalog's thumbnails as CSS. With sheets in the manifest every look (or piece) is cut from one file, so a grid of
// hundreds costs one download; a catalog from before sheets, a list with no sheet, or a sheet that can't be loaded
// shows each entry from its own `thumb` file instead. Draw the answer with `CatalogThumb`.
import { useCallback, useMemo, type CSSProperties } from 'react';
import type { CatalogLook, CatalogPiece } from '../../ipc/schema';
import { sheetCell, sheetOf, wholeThumb, type SheetList } from './thumbSheet';
import { fileFailed, type Catalog } from './useCatalog';

/** A sheet under a short address, by hash. A sheet is named by the CSS of every thumbnail cut from it: hundreds of
 *  elements, which must not each carry the whole picture as a data URL. Kept for the session, as the files are. */
const sheetUrls = new Map<string, string>();

function sheetUrl(sha256: string, dataUrl: string): string {
  let url = sheetUrls.get(sha256);
  if (url === undefined) {
    url = objectUrl(dataUrl) ?? dataUrl;
    sheetUrls.set(sha256, url);
  }
  return url;
}

/** The data URL's bytes behind a `blob:` address, or null where that can't be made (then the data URL itself is used). */
function objectUrl(dataUrl: string): string | null {
  try {
    const comma = dataUrl.indexOf(',');
    const head = dataUrl.slice(0, comma);
    if (typeof URL.createObjectURL !== 'function' || !head.startsWith('data:') || !head.endsWith(';base64')) return null;
    const bytes = Uint8Array.from(atob(dataUrl.slice(comma + 1)), (c) => c.charCodeAt(0));
    return URL.createObjectURL(new Blob([bytes], { type: head.slice(5, -7) }));
  } catch {
    return null;
  }
}

export interface Thumbs {
  /** The CSS that draws the look's thumbnail in a box twice as tall as it is wide, or null while it loads (asking
   *  starts the load, and the component that called `useCatalog` draws again when it lands). The same object on every
   *  draw while nothing changed. */
  look(look: CatalogLook): CSSProperties | null;
  /** The same for a piece, from the pieces' sheet. */
  piece(piece: CatalogPiece): CSSProperties | null;
}

/** Call it in the component that called `useCatalog`, with its answer. */
export function useThumbs(catalog: Catalog): Thumbs {
  const { manifest, image } = catalog;
  // Where each entry sits in its list (retired ones count: the sheets hold a cell for every entry), and the styles
  // already made.
  const places = useMemo(() => {
    const index = (list: readonly { id: string }[]) => new Map(list.map((entry, i) => [entry.id, i]));
    return { looks: index(manifest?.looks ?? []), pieces: index(manifest?.pieces ?? []), styles: new Map<string, { url: string; style: CSSProperties }>() };
  }, [manifest]);

  const thumb = useCallback(
    (list: SheetList, entry: { id: string; thumb: string }): CSSProperties | null => {
      if (!manifest) return null;
      const sheet = sheetOf(manifest, list);
      const index = places[list].get(entry.id);
      const cut = sheet !== null && index !== undefined && !fileFailed(sheet.file);
      const data = image(cut ? sheet.file : entry.thumb);
      if (data === undefined) return null;
      const url = cut ? sheetUrl(sheet.file, data) : data;
      const key = `${list}:${entry.id}`;
      const made = places.styles.get(key);
      if (made?.url === url) return made.style;
      const style = cut ? sheetCell(url, sheet.columns, manifest[list].length, index) : wholeThumb(url);
      places.styles.set(key, { url, style });
      return style;
    },
    [manifest, places, image],
  );

  return useMemo(() => ({ look: (look) => thumb('looks', look), piece: (piece) => thumb('pieces', piece) }), [thumb]);
}
