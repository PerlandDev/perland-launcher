import { act, waitFor } from '@testing-library/react';
import { createElement } from 'react';
import { expect, it } from 'vitest';
import { mockCatalog } from '../../ipc/mockCatalog';
import { renderWithProviders } from '../../test/render';
import { useCatalog, type Catalog } from './useCatalog';

function harness(ipc: (cmd: string, args?: unknown) => unknown) {
  const got: { catalog?: Catalog; renders: number } = { renders: 0 };
  function Harness() {
    got.catalog = useCatalog();
    got.renders += 1;
    return null;
  }
  const result = renderWithProviders(createElement(Harness), { ipc });
  return { got, ...result };
}

it('a cached catalog shows at once and the background refresh brings the new one', async () => {
  const { manifest } = mockCatalog();
  const newer = { ...manifest, version: 'v-newer' };
  let asked = 0;
  const { got } = harness((cmd) => {
    if (cmd !== 'skin_catalog') return null;
    asked += 1;
    return asked === 1 ? { manifest, status: 'refreshing' } : { manifest: newer, status: 'fresh' };
  });
  await waitFor(() => expect(got.catalog!.manifest?.version).toBe(manifest.version));
  expect(got.catalog!.status).toBe('refreshing');
  await waitFor(() => expect(got.catalog!.manifest?.version).toBe('v-newer'), { timeout: 3000 });
  expect(got.catalog!.status).toBe('fresh');
  // Once the answer is in, it stops asking.
  const after = asked;
  await new Promise((r) => setTimeout(r, 1200));
  expect(asked).toBe(after);
});

it('without a cache it waits for the answer and asks once', async () => {
  const { manifest } = mockCatalog();
  let asked = 0;
  const { got } = harness((cmd) => {
    if (cmd !== 'skin_catalog') return null;
    asked += 1;
    return { manifest, status: 'fresh' };
  });
  await waitFor(() => expect(got.catalog!.status).toBe('fresh'));
  await new Promise((r) => setTimeout(r, 1200));
  expect(asked).toBe(1);
});

it('a thumbnail that failed to load is not asked for again this session', async () => {
  const { manifest } = mockCatalog();
  const thumb = 'f'.repeat(64);
  let fileAsks = 0;
  const { got } = harness((cmd) => {
    if (cmd === 'skin_catalog') return { manifest, status: 'fresh' };
    if (cmd === 'skin_catalog_file') {
      fileAsks += 1;
      return Promise.reject({ category: 'network', code: 'skin_catalog_unavailable', message: 'no' });
    }
    return null;
  });
  await waitFor(() => expect(got.catalog!.status).toBe('fresh'));
  expect(got.catalog!.image(thumb)).toBeUndefined();
  await waitFor(() => expect(fileAsks).toBe(1));
  await act(() => new Promise((r) => setTimeout(r, 50)));
  // Every draw asks for it again.
  for (let i = 0; i < 3; i++) {
    expect(got.catalog!.image(thumb)).toBeUndefined();
    await act(() => new Promise((r) => setTimeout(r, 20)));
  }
  await act(() => new Promise((r) => setTimeout(r, 50)));
  expect(fileAsks).toBe(1);
});
