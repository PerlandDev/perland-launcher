import { expect, test } from 'vitest';
import type { CatalogManifest } from '../../ipc/schema';
import { sheetCell, sheetHead, sheetOf, wholeHead, wholeThumb } from './thumbSheet';

const manifest = (sheets?: CatalogManifest['sheets']): CatalogManifest => ({
  formatVersion: 1, version: 'v', license: 'CC0-1.0', slots: [], tags: [], pieces: [], looks: [], expressions: [], sheets,
});

test('the first cell sits at the top left of a sheet scaled by its columns and rows', () => {
  // 50 entries, 24 a row: three rows.
  expect(sheetCell('blob:sheet', 24, 50, 0)).toEqual({
    backgroundImage: 'url("blob:sheet")',
    backgroundRepeat: 'no-repeat',
    backgroundSize: '2400% 300%',
    backgroundPosition: '0% 0%',
    imageRendering: 'pixelated',
  });
});

test('a cell is placed by its column and row, whatever size it is shown at', () => {
  // Entry 25 is the second cell of the second row; entry 49 the second of the third (last) row.
  expect(sheetCell('u', 24, 50, 25).backgroundPosition).toBe(`${(1 / 23) * 100}% 50%`);
  expect(sheetCell('u', 24, 50, 49).backgroundPosition).toBe(`${(1 / 23) * 100}% 100%`);
  // The last cell of a full row.
  expect(sheetCell('u', 24, 24, 23)).toMatchObject({ backgroundSize: '2400% 100%', backgroundPosition: '100% 0%' });
});

test('a sheet of one column or one row never divides by zero', () => {
  expect(sheetCell('u', 1, 3, 2)).toMatchObject({ backgroundSize: '100% 300%', backgroundPosition: '0% 100%' });
  expect(sheetCell('u', 8, 1, 0)).toMatchObject({ backgroundSize: '800% 100%', backgroundPosition: '0% 0%' });
});

test('a thumbnail of its own fills its box', () => {
  expect(wholeThumb('data:image/png;base64,AAAA')).toEqual({
    backgroundImage: 'url("data:image/png;base64,AAAA")',
    backgroundRepeat: 'no-repeat',
    backgroundSize: '100% 100%',
    backgroundPosition: '0% 0%',
    imageRendering: 'pixelated',
  });
});

test('the head of a thumbnail of its own is the middle half of its top quarter, in a square box', () => {
  // The box shows 24 of the thumbnail's 48 x 96 px: the picture is twice as wide and four times as tall as the box.
  expect(wholeHead('u')).toEqual({
    backgroundImage: 'url("u")',
    backgroundRepeat: 'no-repeat',
    backgroundSize: '200% 400%',
    backgroundPosition: '50% 0%',
    imageRendering: 'pixelated',
  });
});

test('the head of a cell is found in its sheet, whatever size it is shown at', () => {
  // 50 entries, 24 a row: a sheet of 1152 x 288 px, of which the box shows 24 x 24.
  expect(sheetHead('blob:sheet', 24, 50, 0)).toEqual({
    backgroundImage: 'url("blob:sheet")',
    backgroundRepeat: 'no-repeat',
    backgroundSize: '4800% 1200%',
    backgroundPosition: `${(12 / 1128) * 100}% 0%`,
    imageRendering: 'pixelated',
  });
  // Entry 25 is the second cell of the second row: its head starts 48 + 12 px in and 96 px down.
  expect(sheetHead('u', 24, 50, 25).backgroundPosition).toBe(`${(60 / 1128) * 100}% ${(96 / 264) * 100}%`);
  // The last cell of the last row stops 12 px short of the right edge, and 72 px short of the bottom.
  expect(sheetHead('u', 2, 4, 3)).toMatchObject({ backgroundSize: '400% 800%', backgroundPosition: `${(60 / 72) * 100}% ${(96 / 168) * 100}%` });
});

test('a manifest has a sheet only when it names a file and at least one column', () => {
  const sheet = (file: string, columns: number) => ({ file, columns });
  expect(sheetOf(manifest(), 'looks')).toBeNull();
  expect(sheetOf(manifest({ pieces: sheet('p', 24), looks: sheet('', 0) }), 'looks')).toBeNull();
  expect(sheetOf(manifest({ pieces: sheet('p', 24), looks: sheet('l', 0) }), 'looks')).toBeNull();
  expect(sheetOf(manifest({ pieces: sheet('p', 24), looks: sheet('l', 12) }), 'looks')).toEqual({ file: 'l', columns: 12 });
  expect(sheetOf(manifest({ pieces: sheet('p', 24), looks: sheet('l', 12) }), 'pieces')).toEqual({ file: 'p', columns: 24 });
});
