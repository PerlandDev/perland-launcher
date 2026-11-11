import { waitFor } from '@testing-library/react';
import { createElement } from 'react';
import { expect, it } from 'vitest';
import { mockCatalog } from '../../ipc/mockCatalog';
import type { CatalogManifest } from '../../ipc/schema';
import { manyLooks } from '../../test/manyLooks';
import { renderWithProviders } from '../../test/render';
import { useCatalog, type Catalog } from './useCatalog';
import { useThumbs, type Thumbs } from './useThumbs';

function harness(ipc: (cmd: string, args?: unknown) => unknown) {
  const got: { catalog?: Catalog; thumbs?: Thumbs } = {};
  function Harness() {
    got.catalog = useCatalog();
    got.thumbs = useThumbs(got.catalog);
    return null;
  }
  return { got, ...renderWithProviders(createElement(Harness), { ipc }) };
}

const SHEET = 'a'.repeat(64);
const PNG = 'data:image/png;base64,AAAA';

/** A catalog of `count` looks whose files are all `PNG`; every file asked for is recorded. */
function thumbsHarness(count: number, sheets: CatalogManifest['sheets'], failing: ReadonlySet<string> = new Set()) {
  const manifest = { ...manyLooks(mockCatalog().manifest, count), sheets };
  const files: string[] = [];
  const result = harness((cmd, args) => {
    if (cmd === 'skin_catalog') return { manifest, status: 'fresh' };
    if (cmd !== 'skin_catalog_file') return null;
    const { sha256 } = args as { sha256: string };
    files.push(sha256);
    return failing.has(sha256) ? Promise.reject({ category: 'network', code: 'skin_catalog_unavailable', message: 'no' }) : PNG;
  });
  return { manifest, files, ...result };
}
const sheets = (looks: string, columns = 24): CatalogManifest['sheets'] => ({ pieces: { file: 'b'.repeat(64), columns: 24 }, looks: { file: looks, columns } });

it('one sheet download serves the thumbnail of every look', async () => {
  const { manifest, files, got } = thumbsHarness(400, sheets(SHEET));
  await waitFor(() => expect(got.catalog!.status).toBe('fresh'));
  // Nothing is drawn until the sheet is here, and asking 400 times starts one download.
  expect(manifest.looks.map((look) => got.thumbs!.look(look))).toEqual(Array(400).fill(null));
  await waitFor(() => expect(got.thumbs!.look(manifest.looks[0]!)).not.toBeNull());
  const styles = manifest.looks.map((look) => got.thumbs!.look(look)!);
  expect(files).toEqual([SHEET]);
  // 400 looks, 24 a row: 17 rows; look 25 is the second cell of the second row.
  expect(styles[0]).toMatchObject({ backgroundSize: '2400% 1700%', backgroundPosition: '0% 0%' });
  expect(styles[25]!.backgroundPosition).toBe(`${(1 / 23) * 100}% ${(1 / 16) * 100}%`);
  expect(new Set(styles.map((s) => s.backgroundPosition)).size).toBe(400);
  // Every cell names the one picture, by an address short enough to repeat 400 times.
  expect(new Set(styles.map((s) => s.backgroundImage)).size).toBe(1);
  expect(String(styles[0]!.backgroundImage)).toMatch(/^url\("blob:[^"]{1,100}"\)$/);
  // The same object every draw, so a tile that hasn't changed isn't drawn again.
  expect(got.thumbs!.look(manifest.looks[25]!)).toBe(styles[25]);
});

it('retired entries keep their cell: the sheet counts every entry of the list', async () => {
  const { manifest, got } = thumbsHarness(3, sheets(SHEET, 2));
  manifest.looks[0]!.retired = true;
  await waitFor(() => expect(got.thumbs!.look(manifest.looks[1]!)).not.toBeNull());
  expect(got.thumbs!.look(manifest.looks[1]!)).toMatchObject({ backgroundSize: '200% 200%', backgroundPosition: '100% 0%' });
  expect(got.thumbs!.look(manifest.looks[2]!)).toMatchObject({ backgroundPosition: '0% 100%' });
});

it('pieces read their own sheet', async () => {
  const { manifest, files, got } = thumbsHarness(3, sheets(SHEET));
  await waitFor(() => expect(got.thumbs!.piece(manifest.pieces[2]!)).not.toBeNull());
  expect(files).toEqual(['b'.repeat(64)]);
  expect(got.thumbs!.piece(manifest.pieces[2]!)).toMatchObject({ backgroundPosition: `${(2 / 23) * 100}% 0%` });
});

it('a piece shown by its head is cut from the same sheet, one download for both views', async () => {
  const { manifest, files, got } = thumbsHarness(3, sheets(SHEET));
  const piece = manifest.pieces[2]!;
  await waitFor(() => expect(got.thumbs!.head(piece)).not.toBeNull());
  // 22 pieces, 24 a row: one row of 1152 x 96 px; the third head starts 2 x 48 + 12 px in.
  expect(got.thumbs!.head(piece)).toMatchObject({ backgroundSize: '4800% 400%', backgroundPosition: `${(108 / 1128) * 100}% 0%` });
  expect(got.thumbs!.piece(piece)).toMatchObject({ backgroundSize: '2400% 100%' });
  expect(files).toEqual(['b'.repeat(64)]);
  // Each view keeps its own style, the same object on every draw.
  expect(got.thumbs!.head(piece)).toBe(got.thumbs!.head(piece));
  expect(got.thumbs!.head(piece)).not.toBe(got.thumbs!.piece(piece));
});

it('without a sheet, a head is cut from the piece\'s own thumbnail', async () => {
  const { manifest, files, got } = thumbsHarness(3, undefined);
  const piece = manifest.pieces[0]!;
  await waitFor(() => expect(got.thumbs!.head(piece)).not.toBeNull());
  expect(got.thumbs!.head(piece)).toMatchObject({ backgroundImage: `url("${PNG}")`, backgroundSize: '200% 400%', backgroundPosition: '50% 0%' });
  expect(files).toEqual([piece.thumb]);
});

it.each([
  ['a catalog from before sheets', undefined],
  ['a sheet with no file', sheets('', 0)],
])('%s shows each thumbnail from its own file', async (_name, withSheets) => {
  const { manifest, files, got } = thumbsHarness(3, withSheets);
  const [first, second] = manifest.looks;
  await waitFor(() => expect(got.thumbs!.look(first!)).not.toBeNull());
  expect(got.thumbs!.look(first!)).toMatchObject({ backgroundImage: `url("${PNG}")`, backgroundSize: '100% 100%' });
  await waitFor(() => expect(got.thumbs!.look(second!)).not.toBeNull());
  expect(files).toEqual([first!.thumb, second!.thumb]);
});

it('a sheet that fails to load falls back to the thumbnail files', async () => {
  const broken = 'c'.repeat(64);
  const { manifest, files, got } = thumbsHarness(3, sheets(broken), new Set([broken]));
  const first = manifest.looks[0]!;
  await waitFor(() => expect(got.thumbs!.look(first)).toMatchObject({ backgroundSize: '100% 100%' }));
  // The sheet was tried once, then the look's own file.
  expect(files).toEqual([broken, first.thumb]);
});
