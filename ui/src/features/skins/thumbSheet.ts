// The catalog's thumbnail sheets: one PNG holding the thumbnail of every entry of a list (`manifest.sheets`). Cell `i`
// (from 0, left to right, then down, `columns` a row, each 48 x 96 px) is the thumbnail of the list's entry `i`, retired
// entries included. This module only does the arithmetic: the CSS that shows one cell in a box of any size, as long as
// the box is twice as tall as it is wide, and the CSS that shows just the cell's head in a square box.
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

/** The head in a thumbnail, in px: a front view at 3 px a texel, so the middle half of its top quarter. */
const HEAD_LEFT = 12;
const HEAD_SIZE = 24;

/** Where a picture `length` px long is placed so the head, `at` px in, starts the box. */
const headAt = (at: number, length: number) => (length > HEAD_SIZE ? (at / (length - HEAD_SIZE)) * 100 : 0);

/** The CSS that shows only the head of cell `index`, filling a square box of any size: what a pair of eyes or a hat
 *  is shown by, so it isn't a speck on a whole figure. */
export function sheetHead(url: string, columns: number, count: number, index: number): CSSProperties {
  const width = columns * THUMB_WIDTH;
  const height = Math.max(1, Math.ceil(count / columns)) * THUMB_HEIGHT;
  return {
    ...base(url),
    backgroundSize: `${(width / HEAD_SIZE) * 100}% ${(height / HEAD_SIZE) * 100}%`,
    backgroundPosition: `${headAt((index % columns) * THUMB_WIDTH + HEAD_LEFT, width)}% ${headAt(Math.floor(index / columns) * THUMB_HEIGHT, height)}%`,
  };
}

/** The same square box showing the head of a thumbnail that is a file of its own. */
export const wholeHead = (url: string): CSSProperties => sheetHead(url, 1, 1, 0);
