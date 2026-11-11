import { act, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import { mockCatalog } from '../../ipc/mockCatalog';
import type { CatalogManifest, CatalogStatus, SkinEntry } from '../../ipc/schema';
import { sent } from '../../test/backend';
import { manyLooks, NEW_LOOKS } from '../../test/manyLooks';
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

/** `looks` swaps the mock's three looks for that many (see `manyLooks`); `manifest` changes what the catalog answers. */
function renderPanel({ status, looks, manifest }: { status?: CatalogStatus; looks?: number; manifest?: (from: CatalogManifest) => CatalogManifest } = {}) {
  const backend = createMockBackend();
  const skins = backend.state.skins;
  if (status) skins.catalogStatus = status;
  if (looks !== undefined) skins.catalog.manifest = manyLooks(skins.catalog.manifest, looks);
  if (manifest) skins.catalog.manifest = manifest(skins.catalog.manifest);
  const calls: [string, unknown][] = [];
  const onAdded = vi.fn<(entry: SkinEntry) => void>();
  const onCustomize = vi.fn<(entry: SkinEntry) => void>();
  const result = renderWithProviders(<CatalogPanel owner={OWNER} onAdded={onAdded} onCustomize={onCustomize} />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      return backend.handle(cmd, args);
    },
  });
  return { ...result, backend, calls, onAdded, onCustomize, manifest: skins.catalog.manifest };
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const grid = () => screen.findByRole('list', { name: 'Looks' });
const names = (list: HTMLElement) => within(list).getAllByRole('button').map((b) => b.getAttribute('aria-label') ?? b.textContent);
const tagKeys = () => within(screen.getByRole('group', { name: 'Tags' })).getAllByRole('button', { pressed: undefined }).filter((b) => b.classList.contains('chip')).map((b) => b.textContent);
/** Opens the page of a look of the grid. */
async function openLook(user: ReturnType<typeof renderPanel>['user'], name: string) {
  await user.click(within(await grid()).getByRole('button', { name }));
  return screen.getByRole('region', { name });
}
const position = () => screen.getByRole('region').querySelector('.look-page-at')?.textContent;
const tagKey = (name: string) => within(screen.getByRole('group', { name: 'Tags' })).getByRole('button', { name: new RegExp(`^${name} \\d+$`) });

it('shows looks and filters by tag', async () => {
  const { user } = renderPanel();
  const list = await grid();
  expect(names(list)).toEqual(['Astronaut', 'Knight', 'Street']);
  // "All" leads, then the tags that have a look, each with how many.
  expect(tagKeys()).toEqual(['All 3', 'Casual 1', 'Fantasy 1', 'Jobs 1', 'New 3', 'Sci-fi 1']);
  expect(tagKey('All')).toHaveAttribute('aria-pressed', 'true');
  expect(screen.getByRole('status')).toHaveTextContent('3 looks');
  await user.click(tagKey('Casual'));
  expect(names(list)).toEqual(['Street']);
  expect(screen.getByRole('status')).toHaveTextContent('1 look');
  await user.click(tagKey('Fantasy'));
  expect(names(list)).toEqual(['Knight']);
  await user.click(tagKey('All'));
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

it('searching narrows the grid and the tag keys', async () => {
  const { user } = renderPanel();
  const list = await grid();
  const search = screen.getByRole('searchbox', { name: 'Search looks' });
  await user.type(search, 'astro');
  expect(names(list)).toEqual(['Astronaut']);
  expect(tagKeys()).toEqual(['All 1', 'Jobs 1', 'New 1', 'Sci-fi 1']);
  await user.clear(search);
  await user.type(search, 'zzz');
  expect(within(list).queryAllByRole('button')).toEqual([]);
  expect(screen.getByText('No looks match that search.')).toBeInTheDocument();
  expect(screen.getByRole('status')).toHaveTextContent('0 looks');
  expect(screen.getByRole('button', { name: 'Random look' })).toBeDisabled();
  // One key puts everything back.
  await user.click(screen.getByRole('button', { name: 'Clear search and filters' }));
  expect(search).toHaveValue('');
  expect(names(list)).toEqual(['Astronaut', 'Knight', 'Street']);
});

it('a search that empties the chosen tag keeps its key, to let it go', async () => {
  const { user } = renderPanel();
  await grid();
  await user.click(tagKey('Fantasy'));
  await user.type(screen.getByRole('searchbox', { name: 'Search looks' }), 'astro');
  expect(tagKeys()).toEqual(['All 1', 'Fantasy 0', 'Jobs 1', 'New 1', 'Sci-fi 1']);
  expect(tagKey('Fantasy')).toHaveAttribute('aria-pressed', 'true');
});

it('filters by arms and orders by name', async () => {
  const { user } = renderPanel();
  const list = await grid();
  await user.selectOptions(screen.getByRole('combobox', { name: 'Arms' }), 'Slim');
  expect(names(list)).toEqual(['Street']);
  expect(tagKeys()).toEqual(['All 1', 'Casual 1', 'New 1']);
  await user.selectOptions(screen.getByRole('combobox', { name: 'Arms' }), 'Classic');
  expect(names(list)).toEqual(['Astronaut', 'Knight']);
  await user.selectOptions(screen.getByRole('combobox', { name: 'Arms' }), 'All');
  expect(within(screen.getByRole('combobox', { name: 'Order' })).getAllByRole('option').map((o) => o.textContent)).toEqual(['Featured', 'A to Z']);
  await user.selectOptions(screen.getByRole('combobox', { name: 'Order' }), 'A to Z');
  expect(names(list)).toEqual(['Astronaut', 'Knight', 'Street']);
});

it('with nothing searched or filtered, a shelf per tag stands over the grid', async () => {
  const { user } = renderPanel();
  const list = await grid();
  const shelves = screen.getByRole('group', { name: 'Looks by tag' });
  // New leads; then the catalog's order.
  expect(within(shelves).getAllByRole('heading').map((h) => h.textContent)).toEqual(['New 3', 'Casual 1', 'Fantasy 1', 'Jobs 1', 'Sci-fi 1']);
  expect(names(within(shelves).getByRole('list', { name: 'New 3' }))).toEqual(['Astronaut', 'Knight', 'Street']);
  expect(names(within(shelves).getByRole('list', { name: 'Fantasy 1' }))).toEqual(['Knight']);
  expect(screen.getByRole('heading', { name: 'All looks 3' })).toBeInTheDocument();
  // See all chooses the shelf's tag: only the grid is left.
  await user.click(within(shelves).getByRole('button', { name: 'See all Fantasy looks' }));
  expect(screen.queryByRole('group', { name: 'Looks by tag' })).not.toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: /^All looks/ })).not.toBeInTheDocument();
  expect(tagKey('Fantasy')).toHaveAttribute('aria-pressed', 'true');
  expect(names(list)).toEqual(['Knight']);
  // A search or the arms hide the shelves too; the order alone doesn't.
  await user.click(tagKey('All'));
  expect(screen.getByRole('group', { name: 'Looks by tag' })).toBeInTheDocument();
  await user.selectOptions(screen.getByRole('combobox', { name: 'Order' }), 'A to Z');
  expect(screen.getByRole('group', { name: 'Looks by tag' })).toBeInTheDocument();
  await user.selectOptions(screen.getByRole('combobox', { name: 'Arms' }), 'Classic');
  expect(screen.queryByRole('group', { name: 'Looks by tag' })).not.toBeInTheDocument();
  await user.selectOptions(screen.getByRole('combobox', { name: 'Arms' }), 'All');
  await user.type(screen.getByRole('searchbox', { name: 'Search looks' }), 'k');
  expect(screen.queryByRole('group', { name: 'Looks by tag' })).not.toBeInTheDocument();
});

it('400 looks: counts, shelves of twelve and one sheet for every thumbnail', async () => {
  const { calls, manifest } = renderPanel({ looks: 400 });
  const list = await grid();
  expect(screen.getByRole('status')).toHaveTextContent('400 looks');
  expect(tagKey('All')).toHaveTextContent('All 400');
  expect(tagKey('New')).toHaveTextContent(`New ${NEW_LOOKS}`);
  // 14 tags share the looks in turn: the first eight get 29, the rest 28.
  expect(tagKey('Adventure')).toHaveTextContent('Adventure 29');
  expect(tagKey('Sport')).toHaveTextContent('Sport 28');
  const shelves = screen.getByRole('group', { name: 'Looks by tag' });
  expect(within(shelves).getAllByRole('heading')).toHaveLength(15);
  const fresh = within(shelves).getByRole('list', { name: `New ${NEW_LOOKS}` });
  expect(names(fresh)).toHaveLength(12);
  expect(names(fresh)[0]).toBe('Amber look 000');
  expect(names(within(shelves).getByRole('list', { name: 'Animals 29' })).slice(0, 2)).toEqual(['Brave look 001', 'Frosty look 015']);
  // The grid draws its first page only.
  expect(names(list)).toHaveLength(60);
  // Every tile on screen (15 shelves of 12, a grid of 60) has its picture once the one sheet is here.
  const tiles = screen.getAllByRole('button', { name: / look \d{3}$/ });
  expect(tiles).toHaveLength(15 * 12 + 60);
  await waitFor(() => expect(tiles.every((tile) => tile.querySelector<HTMLElement>('.skin-thumb')!.style.backgroundImage !== '')).toBe(true));
  const asked = (sent(calls, 'skin_catalog_file') as { sha256: string }[]).map((f) => f.sha256);
  expect(asked.filter((sha256) => sha256 === manifest.sheets!.looks.file)).toHaveLength(1);
  expect(asked.filter((sha256) => manifest.looks.some((look) => look.thumb === sha256))).toEqual([]);
});

it('a tile keeps its place before its picture arrives', async () => {
  renderPanel({ looks: 400 });
  const list = await grid();
  // The box is there, empty, from the first draw: the picture fills it without moving anything.
  const boxes = within(list).getAllByRole('button').map((tile) => tile.querySelector<HTMLElement>('.skin-thumb'));
  expect(boxes.every((box) => box !== null && box.classList.contains('look-tile-thumb'))).toBe(true);
});

it('Show more draws the next page, until every look is drawn', async () => {
  const { user } = renderPanel({ looks: 150 });
  const list = await grid();
  expect(names(list)).toHaveLength(60);
  await user.click(screen.getByRole('button', { name: 'Show more looks' }));
  expect(names(list)).toHaveLength(120);
  expect(names(list)[119]).toBe('Jolly look 119');
  await user.click(screen.getByRole('button', { name: 'Show more looks' }));
  expect(names(list)).toHaveLength(150);
  expect(screen.queryByRole('button', { name: 'Show more looks' })).not.toBeInTheDocument();
});

it('scrolling near the end of the grid draws the next page by itself', async () => {
  const watching = new Set<{ callback: IntersectionObserverCallback }>();
  vi.stubGlobal(
    'IntersectionObserver',
    class {
      callback: IntersectionObserverCallback;
      constructor(callback: IntersectionObserverCallback) {
        this.callback = callback;
      }
      observe() {
        watching.add(this);
      }
      disconnect() {
        watching.delete(this);
      }
    },
  );
  const reachEnd = () => act(() => [...watching].forEach((o) => o.callback([{ isIntersecting: true } as IntersectionObserverEntry], o as unknown as IntersectionObserver)));
  renderPanel({ looks: 150 });
  const list = await grid();
  expect(names(list)).toHaveLength(60);
  expect(watching.size).toBe(1);
  reachEnd();
  expect(names(list)).toHaveLength(120);
  // The end is watched afresh after each page, so one still in view asks again.
  expect(watching.size).toBe(1);
  reachEnd();
  expect(names(list)).toHaveLength(150);
  expect(watching.size).toBe(0);
});

it('a new search starts the grid from its first page again', async () => {
  const { user } = renderPanel({ looks: 150 });
  const list = await grid();
  await user.click(screen.getByRole('button', { name: 'Show more looks' }));
  expect(names(list)).toHaveLength(120);
  await user.type(screen.getByRole('searchbox', { name: 'Search looks' }), 'look');
  expect(screen.getByRole('status')).toHaveTextContent('150 looks');
  expect(names(list)).toHaveLength(60);
});

it('400 looks: the tag, the arms and the order work together', async () => {
  const { user } = renderPanel({ looks: 400 });
  const list = await grid();
  await user.selectOptions(screen.getByRole('combobox', { name: 'Arms' }), 'Slim');
  expect(screen.getByRole('status')).toHaveTextContent('200 looks');
  expect(tagKey('All')).toHaveTextContent('All 200');
  await user.click(tagKey('New'));
  expect(names(list)).toEqual(['Brave look 001', 'Dusty look 003', 'Frosty look 005', 'Happy look 007', 'Jolly look 009', 'Brave look 011', 'Dusty look 013', 'Frosty look 015', 'Happy look 017', 'Jolly look 019']);
  await user.selectOptions(screen.getByRole('combobox', { name: 'Order' }), 'A to Z');
  expect(names(list).slice(0, 3)).toEqual(['Brave look 001', 'Brave look 011', 'Dusty look 003']);
});

it('Random look opens a look of the current results, and then another than the one open', async () => {
  const { user } = renderPanel({ looks: 400 });
  await grid();
  await user.click(tagKey('New'));
  vi.spyOn(Math, 'random').mockReturnValue(0);
  await user.click(screen.getByRole('button', { name: 'Random look' }));
  expect(screen.getByRole('region', { name: 'Amber look 000' })).toBeInTheDocument();
  expect(position()).toBe('1 of 20');
  // From its page: the other 19 are in the draw.
  await user.click(screen.getByRole('button', { name: 'Random look' }));
  expect(screen.getByRole('region', { name: 'Brave look 001' })).toBeInTheDocument();
  vi.spyOn(Math, 'random').mockReturnValue(0.999);
  await user.click(screen.getByRole('button', { name: 'Random look' }));
  expect(screen.getByRole('region', { name: 'Jolly look 019' })).toBeInTheDocument();
  expect(position()).toBe('20 of 20');
});

it("a look's page names it, its tags, its arms and what it wears, piece by piece", async () => {
  const { user } = renderPanel();
  const page = await openLook(user, 'Knight');
  // The looks give way to the page.
  expect(screen.queryByRole('list', { name: 'Looks' })).not.toBeInTheDocument();
  expect(within(page).getByRole('heading', { name: 'Knight' })).toBeInTheDocument();
  expect(within(page).getByText('By Perland · Classic arms')).toBeInTheDocument();
  expect(within(within(page).getByRole('group', { name: 'Tags' })).getAllByRole('button').map((b) => b.textContent)).toEqual(['New', 'Fantasy']);
  const rows = (name: string) => within(within(page).getByRole('list', { name })).getAllByRole('listitem').map((li) => li.textContent);
  // From head to toe, each piece under where it's worn.
  expect(rows('What it wears')).toEqual(["HatKnight's helm", 'TopPlate armour', 'BackCape', 'HandsGloves', 'BottomPlate greaves', 'ShoesBoots']);
  // The body and the face it starts from, apart.
  expect(rows('Body and face')).toEqual(['BodyPlain body', 'EyesRound eyes', 'BrowsStraight brows', 'MouthSmile']);
  // Each piece's colours as the look wears them, named for those who can't see them.
  const cape = within(page).getByRole('list', { name: 'What it wears' }).querySelectorAll('li')[2]!;
  const dots = within(cape).getByRole('img', { name: 'Colours: Main, Trim' });
  expect([...dots.children].map((dot) => (dot as HTMLElement).style.backgroundColor)).toEqual(['rgb(122, 31, 43)', 'rgb(212, 175, 55)']);
  // A piece with no colour area has no dots.
  expect(within(within(page).getByRole('list', { name: 'Body and face' }).querySelectorAll('li')[3]!).queryByRole('img')).not.toBeInTheDocument();
});

it("a look's colours are the look's own, not the pieces' defaults", async () => {
  const { user } = renderPanel({
    manifest: (m) => ({ ...m, looks: m.looks.map((l) => (l.id === 'look-knight' ? { ...l, pieces: l.pieces.map((p) => (p.id === 'back-cape' ? { ...p, colours: { main: '#102030' } } : p)) } : l)) }),
  });
  const page = await openLook(user, 'Knight');
  const dots = within(within(page).getByRole('list', { name: 'What it wears' }).querySelectorAll('li')[2]!).getByRole('img');
  // The area the look doesn't name keeps the piece's default.
  expect([...dots.children].map((dot) => (dot as HTMLElement).style.backgroundColor)).toEqual(['rgb(16, 32, 48)', 'rgb(212, 175, 55)']);
});

it('a tag on the page leads to the looks of that tag', async () => {
  const { user } = renderPanel();
  const page = await openLook(user, 'Knight');
  await user.click(within(page).getByRole('button', { name: 'Fantasy' }));
  expect(screen.queryByRole('region', { name: 'Knight' })).not.toBeInTheDocument();
  expect(tagKey('Fantasy')).toHaveAttribute('aria-pressed', 'true');
  expect(tagKey('Fantasy')).toHaveFocus();
  expect(names(await grid())).toEqual(['Knight']);
});

it('the arrows and the left and right keys step through the looks being browsed', async () => {
  const { user } = renderPanel({ looks: 400 });
  await openLook(user, 'Amber look 000');
  expect(position()).toBe('1 of 400');
  expect(screen.getByRole('button', { name: 'Previous look' })).toBeDisabled();
  await user.click(screen.getByRole('button', { name: 'Next look' }));
  expect(screen.getByRole('region', { name: 'Brave look 001' })).toBeInTheDocument();
  expect(position()).toBe('2 of 400');
  await user.keyboard('{ArrowRight}{ArrowRight}');
  expect(screen.getByRole('region', { name: 'Dusty look 003' })).toBeInTheDocument();
  await user.keyboard('{ArrowLeft}');
  expect(screen.getByRole('region', { name: 'Calm look 002' })).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Previous look' }));
  await user.keyboard('{ArrowLeft}{ArrowLeft}{ArrowLeft}');
  // The first look is as far back as it goes.
  expect(screen.getByRole('region', { name: 'Amber look 000' })).toBeInTheDocument();
  // Its arms follow each look.
  expect(screen.getByText('By Perland · Classic arms')).toBeInTheDocument();
  await user.keyboard('{ArrowRight}');
  expect(screen.getByText('By Perland · Slim arms')).toBeInTheDocument();
});

it('stepping stays inside the search, the tag and the order', async () => {
  const { user } = renderPanel({ looks: 400 });
  await grid();
  await user.click(tagKey('New'));
  await user.selectOptions(screen.getByRole('combobox', { name: 'Arms' }), 'Slim');
  // The page takes the keyboard when it opens.
  expect(await openLook(user, 'Happy look 017')).toHaveFocus();
  expect(position()).toBe('9 of 10');
  await user.click(screen.getByRole('button', { name: 'Next look' }));
  expect(screen.getByRole('region', { name: 'Jolly look 019' })).toBeInTheDocument();
  // At the end the key goes out, and the page takes the keyboard back from it.
  expect(screen.getByRole('button', { name: 'Next look' })).toBeDisabled();
  expect(screen.getByRole('region', { name: 'Jolly look 019' })).toHaveFocus();
  await user.keyboard('{ArrowRight}');
  expect(screen.getByRole('region', { name: 'Jolly look 019' })).toBeInTheDocument();
});

it('back (or Escape) returns to the looks as they were, on the look last open', async () => {
  const { user } = renderPanel({ looks: 150 });
  const list = await grid();
  await user.click(screen.getByRole('button', { name: 'Show more looks' }));
  await user.type(screen.getByRole('searchbox', { name: 'Search looks' }), 'look');
  await user.click(screen.getByRole('button', { name: 'Show more looks' }));
  await openLook(user, 'Amber look 100');
  await user.keyboard('{ArrowRight}');
  await user.click(screen.getByRole('button', { name: 'Back to looks' }));
  expect(screen.queryByRole('region')).not.toBeInTheDocument();
  // The same grid: its search, and the pages already drawn.
  expect(await grid()).toBe(list);
  expect(screen.getByRole('searchbox', { name: 'Search looks' })).toHaveValue('look');
  expect(names(list)).toHaveLength(120);
  expect(within(list).getByRole('button', { name: 'Brave look 101' })).toHaveFocus();
  await user.keyboard('{Enter}');
  expect(screen.getByRole('region', { name: 'Brave look 101' })).toBeInTheDocument();
  await user.keyboard('{Escape}');
  expect(screen.queryByRole('region')).not.toBeInTheDocument();
  expect(within(list).getByRole('button', { name: 'Brave look 101' })).toHaveFocus();
});

it('back from a look opened on a shelf returns to that shelf, not to the grid', async () => {
  const { user } = renderPanel({ looks: 400 });
  await grid();
  const shelf = within(screen.getByRole('group', { name: 'Looks by tag' })).getByRole('list', { name: `New ${NEW_LOOKS}` });
  await user.click(within(shelf).getByRole('button', { name: 'Calm look 002' }));
  expect(screen.getByRole('region', { name: 'Calm look 002' })).toBeInTheDocument();
  await user.keyboard('{Escape}');
  expect(within(shelf).getByRole('button', { name: 'Calm look 002' })).toHaveFocus();
  // Having stepped on, the player is among all the looks: the grid's tile of the look they ended on.
  await user.click(within(shelf).getByRole('button', { name: 'Calm look 002' }));
  await user.keyboard('{ArrowRight}{Escape}');
  expect(within(await grid()).getByRole('button', { name: 'Dusty look 003' })).toHaveFocus();
});

it('add to my wardrobe creates a skin for the shown account, saves its look and stays on the page', async () => {
  const { user, calls, onAdded, onCustomize } = renderPanel();
  const page = await openLook(user, 'Astronaut');
  await user.click(within(page).getByRole('button', { name: 'Add to my wardrobe' }));
  await waitFor(() => expect(onAdded).toHaveBeenCalledTimes(1));
  expect(sent(calls, 'skin_create')).toEqual([{ name: 'Astronaut', model: 'classic', texture: 'data:image/png;base64,ENCODED', owner: OWNER, seeded: false }]);
  const [save] = sent(calls, 'skin_look_save') as { id: string; look: { pieces: { id: string }[]; painting: boolean }; painting: string | null }[];
  const created = onAdded.mock.calls[0]![0];
  expect(save!.id).toBe(created.id);
  expect(save!.look.painting).toBe(false);
  expect(save!.painting).toBeNull();
  expect(save!.look.pieces.map((p) => p.id)).toContain('top-space-suit');
  expect(created.look?.pieces.length).toBe(save!.look.pieces.length);
  // Still on the look, which now says it's the player's; nothing opens in Create.
  expect(onCustomize).not.toHaveBeenCalled();
  expect(within(page).getByText('In your wardrobe')).toBeInTheDocument();
  expect(within(page).queryByRole('button', { name: 'Add to my wardrobe' })).not.toBeInTheDocument();
  // Another look is still to add; coming back to this one, it still isn't.
  await user.keyboard('{ArrowRight}');
  expect(screen.getByRole('button', { name: 'Add to my wardrobe' })).toBeInTheDocument();
  await user.keyboard('{ArrowLeft}');
  expect(screen.getByText('In your wardrobe')).toBeInTheDocument();
});

it('Customize adds the look and hands it on to open in Create', async () => {
  const { user, calls, onAdded, onCustomize } = renderPanel();
  const page = await openLook(user, 'Street');
  await user.click(within(page).getByRole('button', { name: 'Customize' }));
  await waitFor(() => expect(onCustomize).toHaveBeenCalledTimes(1));
  expect(sent(calls, 'skin_create')).toEqual([{ name: 'Street', model: 'slim', texture: 'data:image/png;base64,ENCODED', owner: OWNER, seeded: false }]);
  const [save] = sent(calls, 'skin_look_save') as { id: string }[];
  expect(onCustomize.mock.calls[0]![0].id).toBe(save!.id);
  expect(onAdded).not.toHaveBeenCalled();
});

it('Customize on a look already added opens that skin, without adding it twice', async () => {
  const { user, calls, onAdded, onCustomize } = renderPanel();
  const page = await openLook(user, 'Astronaut');
  await user.click(within(page).getByRole('button', { name: 'Add to my wardrobe' }));
  await waitFor(() => expect(onAdded).toHaveBeenCalledTimes(1));
  await user.click(within(page).getByRole('button', { name: 'Customize' }));
  await waitFor(() => expect(onCustomize).toHaveBeenCalledTimes(1));
  expect(onCustomize.mock.calls[0]![0]).toEqual(onAdded.mock.calls[0]![0]);
  expect(sent(calls, 'skin_create')).toHaveLength(1);
  expect(sent(calls, 'skin_look_save')).toHaveLength(1);
});

it('a look deleted from the wardrobe can be added again', async () => {
  const { user, qc, calls, onAdded } = renderPanel();
  const page = await openLook(user, 'Astronaut');
  await user.click(within(page).getByRole('button', { name: 'Add to my wardrobe' }));
  await waitFor(() => expect(onAdded).toHaveBeenCalledTimes(1));
  const entry = onAdded.mock.calls[0]![0];
  // The Skins screen lists it a moment later; until then it already counts.
  expect(within(page).getByText('In your wardrobe')).toBeInTheDocument();
  // (The list's news reaches the page a tick later, so each change is given its tick.)
  const wardrobe = (entries: SkinEntry[]) => act(() => new Promise<void>((done) => (qc.setQueryData(['skins', OWNER], entries), setTimeout(done, 20))));
  await wardrobe([entry]);
  expect(within(page).getByText('In your wardrobe')).toBeInTheDocument();
  // Deleted there: the page offers it again, and adding makes a new skin.
  await wardrobe([]);
  expect(within(page).queryByText('In your wardrobe')).not.toBeInTheDocument();
  await user.click(within(page).getByRole('button', { name: 'Add to my wardrobe' }));
  await waitFor(() => expect(onAdded).toHaveBeenCalledTimes(2));
  expect(sent(calls, 'skin_create')).toHaveLength(2);
});

it('a failed add says so and leaves the look to add', async () => {
  const { user, backend, onAdded } = renderPanel();
  const page = await openLook(user, 'Astronaut');
  const handle = backend.handle;
  backend.handle = (cmd, args) => (cmd === 'skin_create' ? Promise.reject({ category: 'io', code: 'io_error', message: 'The disk is full.' }) : handle(cmd, args));
  await user.click(within(page).getByRole('button', { name: 'Add to my wardrobe' }));
  await waitFor(() => expect(within(page).getByRole('button', { name: 'Add to my wardrobe' })).not.toHaveAttribute('aria-disabled'));
  expect(onAdded).not.toHaveBeenCalled();
  expect(within(page).queryByText('In your wardrobe')).not.toBeInTheDocument();
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
