import { screen, waitFor, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import { mockCatalog } from '../../ipc/mockCatalog';
import type { CatalogStatus, SkinEntry } from '../../ipc/schema';
import { sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { starterSkin } from './starter';
import { CatalogPanel } from './CatalogPanel';

// jsdom has no canvas or WebGL: textures decode to the starter, encode to a marker, and the preview figure is absent.
vi.mock('./codec', () => ({
  decodeTexture: () => Promise.resolve(starterSkin().data),
  encodeTexture: () => 'data:image/png;base64,ENCODED',
}));
vi.mock('./stageScene', () => ({
  createStage: () => {
    throw new Error('no WebGL');
  },
}));

const OWNER = '5627dd98-e6be-3c21-b8a8-e92344183641';

function renderPanel({ status, onAdded = vi.fn() }: { status?: CatalogStatus; onAdded?: (entry: SkinEntry) => void } = {}) {
  const backend = createMockBackend();
  if (status) backend.state.skins.catalogStatus = status;
  const calls: [string, unknown][] = [];
  const result = renderWithProviders(<CatalogPanel owner={OWNER} onAdded={onAdded} />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      return backend.handle(cmd, args);
    },
  });
  return { ...result, calls, onAdded };
}

const grid = () => screen.findByRole('list', { name: 'Looks' });
const names = (list: HTMLElement) => within(list).getAllByRole('button').map((b) => b.getAttribute('aria-label') ?? b.textContent);

it('shows looks and filters by tag', async () => {
  const { user } = renderPanel();
  const list = await grid();
  expect(names(list)).toEqual(['Astronaut', 'Knight', 'Street']);
  const tags = screen.getByRole('group', { name: 'Tags' });
  // "All" leads, then the manifest's tags.
  expect(within(tags).getAllByRole('button').map((b) => b.textContent)).toEqual([
    'All', 'Adventure', 'Animals', 'Casual', 'Cute', 'Fantasy', 'Formal', 'Heroes', 'History', 'Holidays', 'Jobs', 'Music', 'New', 'Sci-fi', 'Spooky',
    'Sport',
  ]);
  expect(within(tags).getByRole('button', { name: 'All' })).toHaveAttribute('aria-pressed', 'true');
  await user.click(within(tags).getByRole('button', { name: 'Casual' }));
  expect(names(list)).toEqual(['Street']);
  await user.click(within(tags).getByRole('button', { name: 'Sport' }));
  expect(within(list).queryAllByRole('button')).toEqual([]);
  await user.click(within(tags).getByRole('button', { name: 'Fantasy' }));
  expect(names(list)).toEqual(['Knight']);
  await user.click(within(tags).getByRole('button', { name: 'All' }));
  expect(names(list)).toEqual(['Astronaut', 'Knight', 'Street']);
});

it('the grid draws every look from the one thumbnail sheet', async () => {
  const { calls } = renderPanel();
  const { manifest } = mockCatalog();
  const list = await grid();
  await waitFor(() => expect(within(list).getByRole('button', { name: 'Knight' }).querySelector('.skin-thumb')).toHaveStyle({ backgroundPosition: `${(1 / 23) * 100}% 0%` }));
  const asked = (sent(calls, 'skin_catalog_file') as { sha256: string }[]).map((f) => f.sha256);
  expect(asked.filter((sha256) => sha256 === manifest.sheets!.looks.file)).toHaveLength(1);
  // No look's own thumbnail file is fetched.
  expect(asked.filter((sha256) => manifest.looks.some((look) => look.thumb === sha256))).toEqual([]);
});

it('searching narrows the grid', async () => {
  const { user } = renderPanel();
  const list = await grid();
  await user.type(screen.getByRole('searchbox', { name: 'Search looks' }), 'astro');
  expect(names(list)).toEqual(['Astronaut']);
  await user.clear(screen.getByRole('searchbox', { name: 'Search looks' }));
  await user.type(screen.getByRole('searchbox', { name: 'Search looks' }), 'zzz');
  expect(within(list).queryAllByRole('button')).toEqual([]);
  expect(screen.getByText('No looks match that search.')).toBeInTheDocument();
});

it('shows the picked look with what it wears, not the face it starts from', async () => {
  const { user } = renderPanel();
  const list = await grid();
  await user.click(within(list).getByRole('button', { name: 'Knight' }));
  expect(within(list).getByRole('button', { name: 'Knight' })).toHaveAttribute('aria-pressed', 'true');
  const preview = screen.getByRole('region', { name: 'Knight' });
  // Its kind, not that it's new.
  expect(within(preview).getByText('By Perland · Fantasy')).toBeInTheDocument();
  const pieces = within(preview).getByRole('list', { name: 'Pieces' });
  expect(within(pieces).getAllByRole('listitem').map((li) => li.textContent)).toEqual(['Plate greaves', 'Boots', 'Plate armour', 'Gloves', 'Cape', "Knight's helm"]);
});

it('add to my wardrobe creates a skin for the shown account and saves its look', async () => {
  const { user, calls, onAdded } = renderPanel();
  await grid();
  await user.click(screen.getByRole('button', { name: 'Add to my wardrobe' }));
  await waitFor(() => expect(onAdded).toHaveBeenCalledTimes(1));
  expect(sent(calls, 'skin_create')).toEqual([{ name: 'Astronaut', model: 'classic', texture: 'data:image/png;base64,ENCODED', owner: OWNER, seeded: false }]);
  const [save] = sent(calls, 'skin_look_save') as { id: string; look: { pieces: { id: string }[]; painting: boolean }; painting: string | null }[];
  const created = (onAdded as ReturnType<typeof vi.fn>).mock.calls[0]![0] as SkinEntry;
  expect(save!.id).toBe(created.id);
  expect(save!.look.painting).toBe(false);
  expect(save!.painting).toBeNull();
  expect(save!.look.pieces.map((p) => p.id)).toContain('top-space-suit');
  expect(created.look?.pieces.length).toBe(save!.look.pieces.length);
});

it('a cached catalog says so quietly', async () => {
  renderPanel({ status: 'cached' });
  await grid();
  expect(screen.getByText('Showing the saved catalog')).toBeInTheDocument();
});

it('a catalog newer than the launcher asks for an update', async () => {
  renderPanel({ status: 'too_new' });
  await grid();
  expect(screen.getByText('Update the launcher to see the newest pieces.')).toBeInTheDocument();
});

it('an unavailable catalog shows a plain message', async () => {
  renderPanel({ status: 'unavailable' });
  expect(await screen.findByText("Perland's skin catalog can't be reached right now. Try again when you're online.")).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Add to my wardrobe' })).not.toBeInTheDocument();
});
