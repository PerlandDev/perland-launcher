import { expect, test } from 'vitest';
import type { CatalogManifest } from '../../ipc/schema';
import { sheetCell, sheetOf, wholeThumb } from './thumbSheet';

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

test('a manifest has a sheet only when it names a file and at least one column', () => {
  const sheet = (file: string, columns: number) => ({ file, columns });
  expect(sheetOf(manifest(), 'looks')).toBeNull();
  expect(sheetOf(manifest({ pieces: sheet('p', 24), looks: sheet('', 0) }), 'looks')).toBeNull();
  expect(sheetOf(manifest({ pieces: sheet('p', 24), looks: sheet('l', 0) }), 'looks')).toBeNull();
  expect(sheetOf(manifest({ pieces: sheet('p', 24), looks: sheet('l', 12) }), 'looks')).toEqual({ file: 'l', columns: 12 });
  expect(sheetOf(manifest({ pieces: sheet('p', 24), looks: sheet('l', 12) }), 'pieces')).toEqual({ file: 'p', columns: 24 });
});
