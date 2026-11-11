// The catalog's thumbnail sheets: one PNG holding the thumbnail of every entry of a list (`manifest.sheets`). Cell `i`
// (from 0, left to right, then down, `columns` a row, each 48 x 96 px) is the thumbnail of the list's entry `i`, retired
// entries included. This module only does the arithmetic: the CSS that shows one cell in a box of any size, as long as
// the box is twice as tall as it is wide.
import type { CSSProperties } from 'react';
import type { CatalogManifest, CatalogSheet } from '../../ipc/schema';

/** A thumbnail's size in the sheet (and of a thumbnail file of its own), in px: every box that shows one is 1:2. */
export const THUMB_WIDTH = 48;
export const THUMB_HEIGHT = 96;

export type SheetList = 'pieces' | 'looks';

/** The list's sheet, or null when the manifest has none: a catalog from before sheets, or a list with no entries. */
export function sheetOf(manifest: CatalogManifest, list: SheetList): CatalogSheet | null {
  const sheet = manifest.sheets?.[list];
  return sheet && sheet.file !== '' && sheet.columns >= 1 ? sheet : null;
}

const base = (url: string): CSSProperties => ({ backgroundImage: `url("${url}")`, backgroundRepeat: 'no-repeat', imageRendering: 'pixelated' });

/** Where along an axis of `cells` cells cell `at` sits, as a `background-position` percentage. */
const along = (at: number, cells: number) => (cells > 1 ? (at / (cells - 1)) * 100 : 0);

/** The CSS that shows cell `index` of a sheet of `count` entries, `columns` a row. Everything is in percentages of the
 *  box, so the same style fits a thumbnail drawn at any size. */
export function sheetCell(url: string, columns: number, count: number, index: number): CSSProperties {
  const rows = Math.max(1, Math.ceil(count / columns));
  return {
    ...base(url),
    backgroundSize: `${columns * 100}% ${rows * 100}%`,
    backgroundPosition: `${along(index % columns, columns)}% ${along(Math.floor(index / columns), rows)}%`,
  };
}

/** The same box filled by a thumbnail that is a file of its own. */
export function wholeThumb(url: string): CSSProperties {
  return { ...base(url), backgroundSize: '100% 100%', backgroundPosition: '0% 0%' };
}
