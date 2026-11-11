import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import type { SkinLook } from '../../ipc/schema';
import { sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { addFaces, fakeFaceFiles, FAKE_EXPRESSIONS } from '../../test/skinFakes';
import SkinStudio from './SkinStudio';
import { CreatePanel } from './CreatePanel';
import type { Catalog } from './useCatalog';
import type { LookEditor } from './useLook';

vi.mock('./codec', async () => (await import('../../test/skinFakes')).fakeCodec());
vi.mock('./Stage', async () => ({ Stage: (await import('../../test/skinFakes')).FakeStage }));

beforeEach(() => {
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({
    x: 0, y: 0, left: 0, top: 0, right: 256, bottom: 256, width: 256, height: 256, toJSON: () => ({}),
  } as DOMRect);
});
afterEach(() => {
  vi.useRealTimers();
});

const ID = 'skin-0a1b2c3d4e5f';
const TEE = { id: 'top-tee', colours: { main: '#e8e4dc', trim: '#3a6ea5' } };
const BODY = { id: 'body-plain', colours: { skin: '#d9a27c' } };

/** The Skins screen on its first skin, wearing `look`, opened on the Create tab. */
async function openCreate(
  look?: SkinLook,
  {
    catalog = 'fresh',
    microsoft = false,
    files,
    listOnce = false,
    faces = false,
  }: { catalog?: 'fresh' | 'unavailable' | 'never'; microsoft?: boolean; files?: { held: Promise<void> | null }; listOnce?: boolean; faces?: boolean } = {},
) {
  const backend = createMockBackend(microsoft ? { microsoft: 'signedIn' } : {});
  // faces: the catalog also has the test expressions and the pieces they name.
  if (faces) addFaces(backend.state.skins.catalog);
  if (look) backend.state.skins.entries.find((e) => e.id === ID)!.look = look;
  if (catalog === 'unavailable') backend.state.skins.catalogStatus = 'unavailable';
  const calls: [string, unknown][] = [];
  const result = renderWithProviders(<SkinStudio />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      // 'never': the catalog is still loading for the whole test.
      if (cmd === 'skin_catalog' && catalog === 'never') return new Promise(() => {});
      // listOnce: the wardrobe is listed once, and every later listing never answers.
      if (cmd === 'skins_list' && listOnce && calls.filter(([c]) => c === 'skins_list').length > 1) return new Promise(() => {});
      // A held gate keeps the catalog's files loading.
      const held = files?.held;
      if (cmd === 'skin_catalog_file' && held) return held.then(() => backend.handle(cmd, args));
      return backend.handle(cmd, args);
    },
  });
  await screen.findByText('Saved');
  await result.user.click(screen.getByRole('tab', { name: 'Create' }));
  if (catalog === 'fresh') await screen.findByRole('group', { name: 'Categories' });
  return { ...result, calls, backend };
}

const wearing = () => screen.getByRole('list', { name: 'Wearing' });
const chips = () => within(wearing()).queryAllByRole('listitem').map((li) => li.textContent);
const stageSum = () => screen.getByTestId('stage').getAttribute('data-sum');

it('putting on a top replaces the previous top', async () => {
  const { user } = await openCreate({ pieces: [BODY, TEE], painting: false });
  await user.click(screen.getByRole('button', { name: 'Tops' }));
  const grid = screen.getByRole('list', { name: 'Tops' });
  expect(within(grid).getByRole('button', { name: /T-shirt/ })).toHaveTextContent('On');
  await user.click(within(grid).getByRole('button', { name: /Bomber jacket/ }));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'Bomber jacket']));
  expect(within(grid).getByRole('button', { name: /Bomber jacket/ })).toHaveTextContent('On');
  expect(within(grid).getByRole('button', { name: /T-shirt/ })).not.toHaveTextContent('On');
  // The new top is the one being coloured.
  expect(screen.getByRole('region', { name: 'Bomber jacket colours' })).toBeInTheDocument();
});

it('changing an area colour updates the stage texture', async () => {
  const { user } = await openCreate({ pieces: [BODY, TEE], painting: false });
  await user.click(screen.getByRole('button', { name: 'Tops' }));
  await user.click(within(screen.getByRole('list', { name: 'Tops' })).getByRole('button', { name: /T-shirt/ }));
  const colours = screen.getByRole('region', { name: 'T-shirt colours' });
  const before = stageSum();
  const hex = within(colours).getByRole('textbox', { name: 'Hex colour for Main' });
  await user.clear(hex);
  await user.type(hex, '#112233');
  await waitFor(() => expect(stageSum()).not.toBe(before));
  // A quick colour of the area: one click (the fake masks paint the first area only).
  const now = stageSum();
  await user.click(within(within(colours).getByRole('toolbar', { name: 'Main' })).getAllByRole('button')[1]!);
  await waitFor(() => expect(stageSum()).not.toBe(now));
});

it('take off removes the piece and the chip', async () => {
  const { user } = await openCreate({ pieces: [BODY, TEE], painting: false });
  await user.click(screen.getByRole('button', { name: 'Tops' }));
  await user.click(within(screen.getByRole('list', { name: 'Tops' })).getByRole('button', { name: /T-shirt/ }));
  await user.click(screen.getByRole('button', { name: 'Take off' }));
  await waitFor(() => expect(chips()).toEqual(['Plain body']));
  // The chip's × takes a piece off too.
  await user.click(within(wearing()).getByRole('button', { name: 'Take off Plain body' }));
  await waitFor(() => expect(chips()).toEqual([]));
});

it('make part of the skin asks, then leaves no pieces and one undo brings them back', async () => {
  const { user, calls } = await openCreate({ pieces: [BODY, TEE], painting: false });
  vi.useFakeTimers({ shouldAdvanceTime: true });
  await user.click(screen.getByRole('button', { name: 'Make part of the skin' }));
  const dialog = await screen.findByRole('dialog');
  expect(chips()).toEqual(['Plain body', 'T-shirt']);
  await user.click(within(dialog).getByRole('button', { name: 'Make part of the skin' }));
  await waitFor(() => expect(chips()).toEqual([]));
  await vi.advanceTimersByTimeAsync(700);
  await waitFor(() => expect(sent(calls, 'skin_look_save')).toHaveLength(1));
  const [save] = sent(calls, 'skin_look_save') as { look: SkinLook; painting: string | null }[];
  expect(save!.look).toEqual({ pieces: [], painting: true });
  expect(save!.painting).not.toBeNull();
  await user.click(screen.getByRole('button', { name: 'Undo' }));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'T-shirt']));
});

it('a catalog look asks before replacing worn pieces, and keeps the painting', async () => {
  const { user } = await openCreate({ pieces: [BODY, TEE], painting: false });
  await user.click(screen.getByRole('button', { name: 'Looks' }));
  await user.click(within(screen.getByRole('list', { name: 'Looks' })).getByRole('button', { name: 'Knight' }));
  const dialog = await screen.findByRole('dialog');
  await user.click(within(dialog).getByRole('button', { name: 'Put it on' }));
  await waitFor(() => expect(chips()).toContain('Plate armour'));
  expect(chips()).not.toContain('T-shirt');
});

it('build from pieces starts with the plain body and a simple face', async () => {
  const backend = createMockBackend();
  const calls: [string, unknown][] = [];
  const { user } = renderWithProviders(<SkinStudio />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      return backend.handle(cmd, args);
    },
  });
  await screen.findByText('Saved');
  await user.click(screen.getByRole('button', { name: 'New' }));
  await user.click(await screen.findByRole('menuitem', { name: 'Build from pieces' }));
  await waitFor(() => expect(sent(calls, 'skin_look_save')).toHaveLength(1));
  const [created] = sent(calls, 'skin_create') as { name: string; model: string; owner: string | null; texture: string }[];
  expect(created).toMatchObject({ name: 'My character', model: 'classic', owner: '5627dd98-e6be-3c21-b8a8-e92344183641' });
  const [save] = sent(calls, 'skin_look_save') as { look: SkinLook; painting: string | null; texture: string }[];
  expect(save!.look).toEqual({
    pieces: [
      { id: 'body-plain', colours: { skin: '#d9a27c' } },
      { id: 'eyes-round', colours: { iris: '#4a3324' } },
      { id: 'brows-straight', colours: { main: '#3b2618' } },
      { id: 'mouth-smile', colours: {} },
    ],
    painting: false,
  });
  expect(save!.painting).toBeNull();
  expect(save!.texture).toBe(created!.texture);
  // It opens in Create, wearing the body and a face.
  expect(await screen.findByRole('tab', { name: 'Create', selected: true })).toBeInTheDocument();
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'Round eyes', 'Straight brows', 'Smile']));
});

it('a piece missing from the catalog stays worn and is labelled unavailable', async () => {
  const { user } = await openCreate({ pieces: [BODY, { id: 'top-retired-long-ago', colours: {} }], painting: false });
  expect(chips()).toEqual(['Plain body', 'Piece no longer available']);
  // Other changes keep it.
  await user.click(screen.getByRole('button', { name: 'Hats' }));
  await user.click(within(screen.getByRole('list', { name: 'Hats' })).getByRole('button', { name: /^Cap/ }));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'Piece no longer available', 'Cap']));
  await user.click(within(wearing()).getByRole('button', { name: 'Take off Piece no longer available' }));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'Cap']));
});

it('a drag on the figure in Create turns it and paints nothing', async () => {
  const { user, calls } = await openCreate({ pieces: [BODY, TEE], painting: false });
  expect(screen.getByText('Drag the figure, or right-drag, to turn it.')).toBeInTheDocument();
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const before = stageSum();
  fireEvent.pointerDown(screen.getByTestId('stage'));
  await act(() => vi.advanceTimersByTimeAsync(700));
  expect(stageSum()).toBe(before);
  expect(sent(calls, 'skin_save')).toEqual([]);
  expect(sent(calls, 'skin_look_save')).toEqual([]);
  expect(screen.getByRole('button', { name: 'Undo' })).toBeDisabled();
  // The same press in Paint paints.
  await user.click(screen.getByRole('tab', { name: 'Paint' }));
  fireEvent.pointerDown(screen.getByTestId('stage'));
  await waitFor(() => expect(stageSum()).not.toBe(before));
});

it('while the catalog loads, worn pieces wait: no names, no ×', async () => {
  await openCreate({ pieces: [BODY, TEE], painting: false }, { catalog: 'never' });
  await waitFor(() => expect(chips()).toEqual(['Loading…', 'Loading…']));
  for (const x of within(wearing()).getAllByRole('button')) expect(x).toBeDisabled();
  expect(screen.queryByText('Piece no longer available')).not.toBeInTheDocument();
});

it('without the catalog, a skin wearing pieces keeps them and its arms', async () => {
  await openCreate({ pieces: [BODY, TEE], painting: false }, { catalog: 'unavailable' });
  await waitFor(() => expect(chips()).toEqual(['Piece', 'Piece']));
  for (const x of within(wearing()).getAllByRole('button')) expect(x).toBeDisabled();
  const slim = screen.getByRole('radio', { name: 'Slim arms' });
  expect(slim).toBeDisabled();
  expect(slim).toHaveAttribute('title', expect.stringContaining("Perland's catalog"));
});

/** Holds the catalog's file reads while closed. */
function gate() {
  let release: () => void = () => {};
  const g = {
    held: null as Promise<void> | null,
    close() {
      g.held = new Promise((r) => (release = r));
    },
    open() {
      g.held = null;
      release();
    },
  };
  return g;
}

it('Wear during a put-on still loading uploads the skin with the piece', async () => {
  const files = gate();
  const { user, calls } = await openCreate(undefined, { microsoft: true, files });
  await user.click(screen.getByRole('button', { name: 'Accessories' }));
  files.close();
  // No other test here (nor any catalog look) loads the scarf's files, so the put-on waits on them.
  await user.click(within(screen.getByRole('list', { name: 'Neck' })).getByRole('button', { name: /Scarf/ }));
  await user.click(screen.getByRole('button', { name: 'Wear this skin' }));
  await act(() => new Promise((r) => setTimeout(r, 30)));
  expect(sent(calls, 'skin_wear')).toEqual([]);
  files.open();
  await waitFor(() => expect(sent(calls, 'skin_wear')).toHaveLength(1));
  const order = calls.map(([c]) => c).filter((c) => c === 'skin_look_save' || c === 'skin_wear');
  expect(order).toEqual(['skin_look_save', 'skin_wear']);
  const [save] = sent(calls, 'skin_look_save') as { id: string; look: SkinLook }[];
  expect(save!.look.pieces.map((p) => p.id)).toEqual(['neck-scarf']);
  expect(save!.id).toBe((sent(calls, 'skin_wear')[0] as { id: string }).id);
});

it('picking another skin during a change still loading keeps the change', async () => {
  const files = gate();
  const { user, calls } = await openCreate({ pieces: [BODY], painting: false }, { files });
  await user.click(screen.getByRole('button', { name: 'Accessories' }));
  files.close();
  // No other test here (nor any catalog look) loads the glasses' files.
  await user.click(within(screen.getByRole('list', { name: 'Glasses' })).getByRole('button', { name: /Round glasses/ }));
  await user.click(within(screen.getByRole('list', { name: 'Wardrobe' })).getByText('Miner'));
  files.open();
  await waitFor(() => expect(sent(calls, 'skin_look_save')).toHaveLength(1));
  const [save] = sent(calls, 'skin_look_save') as { id: string; look: SkinLook }[];
  expect(save!.id).toBe(ID);
  expect(save!.look.pieces.map((p) => p.id)).toEqual(['body-plain', 'glasses-round']);
});

it('a face piece the painting covers shows the hint, whose button builds from pieces', async () => {
  const { manifest } = (await import('../../ipc/mockCatalog')).mockCatalog();
  const catalog: Catalog = { manifest, status: 'fresh', loading: false, image: () => undefined, pixels: () => new Promise(() => {}), layers: async () => [] };
  const noop = () => {};
  const editor: LookEditor = {
    look: { pieces: [{ id: 'eyes-round', colours: { iris: '#4a3324' } }], painting: true },
    painting: null,
    putOn: noop,
    takeOff: noop,
    setColour: noop,
    applyLook: noop,
    bakeAll: noop,
    switchModel: noop,
    applyExpression: noop,
    restyle: noop,
    paintingCovers: true,
    rev: 0,
    previewFace: () => Promise.resolve(null),
    preview: () => Promise.resolve(null),
    version: () => 0,
    settled: () => Promise.resolve(),
  };
  const build = vi.fn();
  const { user, rerender } = renderWithProviders(<CreatePanel catalog={catalog} look={editor.look} editor={editor} lang="en" onBuildFromPieces={build} />);
  expect(screen.getByText('Your painting covers this piece. Build a new skin from pieces to start clean.')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Build from pieces' }));
  expect(build).toHaveBeenCalledTimes(1);
  // Gone once a change shows.
  rerender(<CreatePanel catalog={catalog} look={editor.look} editor={{ ...editor, paintingCovers: false }} lang="en" onBuildFromPieces={build} />);
  expect(screen.queryByText(/Your painting covers this piece/)).not.toBeInTheDocument();
});

it('a skin reopened before the wardrobe is listed again shows the look it saved', async () => {
  const { user, calls } = await openCreate({ pieces: [BODY, TEE], painting: false }, { listOnce: true });
  await user.click(screen.getByRole('button', { name: 'Hats' }));
  await user.click(within(screen.getByRole('list', { name: 'Hats' })).getByRole('button', { name: /^Cap/ }));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'T-shirt', 'Cap']));
  await waitFor(() => expect(sent(calls, 'skin_look_save')).toHaveLength(1), { timeout: 2000 });
  const list = screen.getByRole('list', { name: 'Wardrobe' });
  await user.click(within(list).getByText('Miner'));
  await user.click(within(list).getByText('Beach day'));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'T-shirt', 'Cap']));
});

const EYES = { id: 'eyes-round', colours: { iris: '#2266cc' } };
const BROWS = { id: 'brows-straight', colours: { main: '#3b2618' } };
const MOUTH = { id: 'mouth-smile', colours: {} };
const expressions = () => screen.getByRole('group', { name: 'Expression' });
const offered = () => within(expressions()).getAllByRole('button').map((b) => b.getAttribute('aria-label'));
const askedFiles = (calls: [string, unknown][]) => (sent(calls, 'skin_catalog_file') as { sha256: string }[]).map((a) => a.sha256);

it('Face offers the catalog\'s expressions in order, loading their pieces only once it is open', async () => {
  const { user, calls } = await openCreate({ pieces: [BODY, EYES, BROWS, MOUTH], painting: false }, { faces: true });
  // Tops is open: no expression row, and none of the expressions' own pieces was loaded.
  expect(screen.queryByRole('group', { name: 'Expression' })).not.toBeInTheDocument();
  expect(askedFiles(calls).filter((f) => f.startsWith('fake-'))).toEqual([]);
  await user.click(screen.getByRole('button', { name: 'Face' }));
  // The one naming a piece no catalog has is left out.
  expect(offered()).toEqual(['Happy', 'Angry', 'Determined']);
  // Above the eyes, brows and mouth.
  expect(expressions().compareDocumentPosition(screen.getByRole('list', { name: 'Eyes' })) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  // The face worn now is marked.
  expect(within(expressions()).getByRole('button', { name: 'Happy' })).toHaveAttribute('aria-current', 'true');
  expect(within(expressions()).getByRole('button', { name: 'Angry' })).not.toHaveAttribute('aria-current');
  // The mini faces loaded the pieces the expressions name, and no other piece of the catalog.
  const named = ['eyes-fierce', 'brows-angry', 'mouth-gritted'].flatMap(fakeFaceFiles).sort();
  await waitFor(() => expect(askedFiles(calls).filter((f) => f.startsWith('fake-')).sort()).toEqual(named));
});

it('an expression chip puts the three pieces on in one step that Undo takes back', async () => {
  const { user } = await openCreate({ pieces: [BODY, EYES, BROWS, MOUTH], painting: false }, { faces: true });
  await user.click(screen.getByRole('button', { name: 'Face' }));
  const before = stageSum();
  await user.click(within(expressions()).getByRole('button', { name: 'Angry' }));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'Fierce eyes', 'Angry brows', 'Gritted teeth']));
  expect(within(expressions()).getByRole('button', { name: 'Angry' })).toHaveAttribute('aria-current', 'true');
  expect(within(expressions()).getByRole('button', { name: 'Happy' })).not.toHaveAttribute('aria-current');
  // The eyes kept the player's blue: the fake eyes draw their iris colour, so the default amber would look different.
  await user.click(screen.getByRole('button', { name: 'Undo' }));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'Round eyes', 'Straight brows', 'Smile']));
  expect(stageSum()).toBe(before);
  expect(screen.getByRole('button', { name: 'Undo' })).toBeDisabled();
});

it('a catalog without expressions shows no expression row', async () => {
  const { user } = await openCreate({ pieces: [BODY, EYES, BROWS, MOUTH], painting: false });
  await user.click(screen.getByRole('button', { name: 'Face' }));
  expect(screen.getByRole('list', { name: 'Eyes' })).toBeInTheDocument();
  expect(screen.queryByRole('group', { name: 'Expression' })).not.toBeInTheDocument();
});

it('each expression chip previews its own face and never changes the skin until it is clicked', async () => {
  const mock = (await import('../../ipc/mockCatalog')).mockCatalog();
  addFaces(mock);
  const catalog: Catalog = { manifest: mock.manifest, status: 'fresh', loading: false, image: () => undefined, pixels: () => new Promise(() => {}), layers: async () => [] };
  const noop = () => {};
  const look: SkinLook = { pieces: [EYES, BROWS, MOUTH], painting: true };
  const previewFace = vi.fn<LookEditor['previewFace']>(() => Promise.resolve(new Uint8ClampedArray(8 * 8 * 4)));
  const applyExpression = vi.fn();
  const editor: LookEditor = {
    look,
    painting: null,
    putOn: noop,
    takeOff: noop,
    setColour: noop,
    applyLook: noop,
    applyExpression,
    restyle: noop,
    bakeAll: noop,
    switchModel: noop,
    paintingCovers: true,
    rev: 0,
    previewFace,
    preview: () => Promise.resolve(null),
    version: () => 0,
    settled: () => Promise.resolve(),
  };
  // Rendered bare, so a rerender keeps the panel (and its open category) mounted.
  const user = userEvent.setup();
  const { rerender } = render(<CreatePanel catalog={catalog} look={look} editor={editor} lang="es" />);
  expect(previewFace).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Face' }));
  expect(offered()).toEqual(['Feliz', 'Enojado', 'Decidido']);
  // One preview per chip: the look with that expression on, in the player's colours.
  await waitFor(() => expect(previewFace).toHaveBeenCalledTimes(3));
  const previewed = previewFace.mock.calls.map(([change]) => change(look).pieces);
  expect(previewed[0]).toEqual(look.pieces);
  expect(previewed[1]).toEqual([{ id: 'eyes-fierce', colours: { iris: '#2266cc' } }, { id: 'brows-angry', colours: { main: '#3b2618' } }, { id: 'mouth-gritted', colours: {} }]);
  expect(applyExpression).not.toHaveBeenCalled();
  // The painting's hint stays with the row.
  expect(screen.getByText(/Your painting covers this piece/)).toBeInTheDocument();
  // A change of the skin draws the faces again; a render with nothing new doesn't.
  rerender(<CreatePanel catalog={catalog} look={look} editor={{ ...editor }} lang="es" />);
  expect(previewFace).toHaveBeenCalledTimes(3);
  rerender(<CreatePanel catalog={catalog} look={look} editor={{ ...editor, rev: 1 }} lang="es" />);
  await waitFor(() => expect(previewFace).toHaveBeenCalledTimes(6));
  await user.click(within(expressions()).getByRole('button', { name: 'Enojado' }));
  expect(applyExpression).toHaveBeenCalledTimes(1);
  expect(applyExpression).toHaveBeenCalledWith(FAKE_EXPRESSIONS[1]);
});

/** `Math.random` answering a fixed run of numbers, so the outfits are the same on every run. */
function fixedDice(seed: number) {
  let a = seed >>> 0;
  return vi.spyOn(Math, 'random').mockImplementation(() => {
    a = (Math.imul(a, 1664525) + 1013904223) >>> 0;
    return a / 4294967296;
  });
}

it('Surprise me dresses the skin in one step, keeps the body, and Undo brings the outfit before back', async () => {
  const body = { id: 'body-plain', colours: { skin: '#3f2a1f' } };
  const { user, calls } = await openCreate({ pieces: [body, TEE], painting: false });
  const dice = fixedDice(42);
  const start = stageSum();
  await user.click(screen.getByRole('button', { name: 'Surprise me' }));
  await waitFor(() => expect(stageSum()).not.toBe(start));
  const first = { chips: chips(), sum: stageSum() };
  // A whole character: the body it had, a face, hair, a top, a bottom and shoes.
  expect(first.chips[0]).toBe('Plain body');
  for (const name of ['Round eyes', 'Straight brows', 'Smile', 'Short hair']) expect(first.chips).toContain(name);
  expect(first.chips.length).toBeGreaterThanOrEqual(8);
  // Again: another outfit.
  await user.click(screen.getByRole('button', { name: 'Surprise me' }));
  await waitFor(() => expect(stageSum()).not.toBe(first.sum));
  const second = stageSum();
  await waitFor(() => expect(sent(calls, 'skin_look_save').length).toBeGreaterThan(0), { timeout: 2000 });
  const saves = sent(calls, 'skin_look_save') as { look: SkinLook }[];
  // The skin tone is the player's own, whatever the dice say.
  expect(saves[saves.length - 1]!.look.pieces[0]).toEqual(body);
  // One Undo per outfit.
  await user.click(screen.getByRole('button', { name: 'Undo' }));
  await waitFor(() => expect(stageSum()).toBe(first.sum));
  expect(chips()).toEqual(first.chips);
  await user.click(screen.getByRole('button', { name: 'Undo' }));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'T-shirt']));
  expect(stageSum()).toBe(start);
  expect(screen.getByRole('button', { name: 'Undo' })).toBeDisabled();
  expect(second).not.toBe(start);
  dice.mockRestore();
});

it('Surprise me needs the catalog: without it the key is not there', async () => {
  await openCreate({ pieces: [BODY, TEE], painting: false }, { catalog: 'unavailable' });
  expect(screen.queryByRole('button', { name: 'Surprise me' })).not.toBeInTheDocument();
});

it('Surprise me is a Create key: Paint has none', async () => {
  const { user } = await openCreate({ pieces: [BODY, TEE], painting: false });
  expect(screen.getByRole('button', { name: 'Surprise me' })).toBeInTheDocument();
  await user.click(screen.getByRole('tab', { name: 'Paint' }));
  expect(screen.queryByRole('button', { name: 'Surprise me' })).not.toBeInTheDocument();
});

// Try before you wear: pointing at a tile shows it on the figure, and only a click changes the skin.
const badge = () => screen.queryByTestId('stage-try');
const tile = (list: string, name: RegExp | string) => within(screen.getByRole('list', { name: list })).getByRole('button', { name });
const fileAsks = (calls: [string, unknown][]) => (sent(calls, 'skin_catalog_file') as { sha256: string }[]).map((a) => a.sha256);

it('pointing at a piece shows it on the figure with a Preview badge and changes nothing; a click wears what was shown', async () => {
  const { user, calls } = await openCreate({ pieces: [BODY, TEE], painting: false });
  await user.click(screen.getByRole('button', { name: 'Shoes' }));
  const real = stageSum();
  vi.useFakeTimers({ shouldAdvanceTime: true });
  fireEvent.pointerEnter(tile('Shoes', /Moon boots/), { pointerType: 'mouse' });
  // Not at once: a pointer passing over shows nothing.
  expect(badge()).toBeNull();
  await waitFor(() => expect(badge()).toHaveTextContent('PreviewMoon boots'));
  const previewed = stageSum();
  expect(previewed).not.toBe(real);
  expect(screen.getByText('Not on yet. Click it to wear it.')).toBeInTheDocument();
  // The skin is as it was: nothing worn, nothing to undo, nothing saved.
  await act(() => vi.advanceTimersByTimeAsync(800));
  expect(chips()).toEqual(['Plain body', 'T-shirt']);
  expect(screen.getByRole('button', { name: 'Undo' })).toBeDisabled();
  expect(screen.getByText('Saved')).toBeInTheDocument();
  expect(sent(calls, 'skin_look_save')).toEqual([]);
  expect(sent(calls, 'skin_save')).toEqual([]);
  expect(tile('Shoes', /Moon boots/)).not.toHaveTextContent('On');
  // Leaving the tile puts the real skin back.
  fireEvent.pointerLeave(tile('Shoes', /Moon boots/));
  await waitFor(() => expect(badge()).toBeNull());
  expect(stageSum()).toBe(real);
  // A click wears it: the figure is what the preview showed, with no badge.
  fireEvent.pointerEnter(tile('Shoes', /Moon boots/), { pointerType: 'mouse' });
  await waitFor(() => expect(badge()).not.toBeNull());
  await user.click(tile('Shoes', /Moon boots/));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'T-shirt', 'Moon boots']));
  await waitFor(() => expect(badge()).toBeNull());
  expect(stageSum()).toBe(previewed);
  expect(screen.getByRole('button', { name: 'Undo' })).toBeEnabled();
});

it('a pointer swept across the grid loads no files for the tiles it crossed', async () => {
  const { user, calls } = await openCreate({ pieces: [BODY], painting: false });
  await user.click(screen.getByRole('button', { name: 'Hats' }));
  const hats = within(screen.getByRole('list', { name: 'Hats' })).getAllByRole('button');
  expect(hats.length).toBeGreaterThan(1);
  await waitFor(() => expect(screen.getByTestId('stage').getAttribute('data-sum')).not.toBe(''));
  const before = fileAsks(calls).length;
  const thumbs = new Set((await import('../../ipc/mockCatalog')).mockCatalog().manifest.pieces.map((p) => p.thumb));
  vi.useFakeTimers();
  for (const hat of hats) {
    fireEvent.pointerEnter(hat, { pointerType: 'mouse' });
    await act(() => vi.advanceTimersByTimeAsync(30));
    fireEvent.pointerLeave(hat);
  }
  await act(() => vi.advanceTimersByTimeAsync(1000));
  expect(badge()).toBeNull();
  // Thumbnails may still be landing; no piece's art or mask was asked for.
  expect(fileAsks(calls).slice(before).filter((sha) => !thumbs.has(sha))).toEqual([]);
});

it('a finger gets no hover preview', async () => {
  const { user } = await openCreate({ pieces: [BODY], painting: false });
  await user.click(screen.getByRole('button', { name: 'Hats' }));
  const real = stageSum();
  vi.useFakeTimers();
  fireEvent.pointerEnter(tile('Hats', /^Cap/), { pointerType: 'touch' });
  await act(() => vi.advanceTimersByTimeAsync(1000));
  expect(badge()).toBeNull();
  expect(stageSum()).toBe(real);
});

it('keyboard focus on a tile tries it on, and Escape goes back to the real skin', async () => {
  const { user } = await openCreate({ pieces: [BODY], painting: false });
  await user.click(screen.getByRole('button', { name: 'Hats' }));
  const real = stageSum();
  const cap = tile('Hats', /^Cap/);
  // Tab from the last category key lands on the first tile of the grid.
  screen.getByRole('button', { name: 'Accessories' }).focus();
  await user.tab();
  expect(within(screen.getByRole('list', { name: 'Hats' })).getAllByRole('button')[0]).toHaveFocus();
  await waitFor(() => expect(badge()).not.toBeNull());
  expect(stageSum()).not.toBe(real);
  await user.keyboard('{Escape}');
  expect(badge()).toBeNull();
  expect(stageSum()).toBe(real);
  expect(chips()).toEqual(['Plain body']);
  // Enter wears the focused piece.
  cap.focus();
  await user.keyboard('{Enter}');
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'Cap']));
});

it('a look and an expression are tried on the same way', async () => {
  const { user } = await openCreate({ pieces: [BODY, EYES, BROWS, MOUTH], painting: false }, { faces: true });
  await user.click(screen.getByRole('button', { name: 'Looks' }));
  const real = stageSum();
  fireEvent.pointerEnter(tile('Looks', 'Knight'), { pointerType: 'mouse' });
  await waitFor(() => expect(badge()).toHaveTextContent('PreviewKnight'));
  const knight = stageSum();
  expect(knight).not.toBe(real);
  // Opening another category leaves the tile behind: the real skin is back at once.
  await user.click(screen.getByRole('button', { name: 'Face' }));
  expect(badge()).toBeNull();
  expect(stageSum()).toBe(real);
  fireEvent.pointerEnter(within(expressions()).getByRole('button', { name: 'Angry' }), { pointerType: 'mouse' });
  await waitFor(() => expect(badge()).toHaveTextContent('PreviewAngry'));
  expect(stageSum()).not.toBe(real);
  expect(stageSum()).not.toBe(knight);
  // The face worn already has nothing to show.
  fireEvent.pointerLeave(within(expressions()).getByRole('button', { name: 'Angry' }));
  fireEvent.pointerEnter(within(expressions()).getByRole('button', { name: 'Happy' }), { pointerType: 'mouse' });
  await waitFor(() => expect(badge()).toBeNull());
  expect(stageSum()).toBe(real);
  expect(chips()).toEqual(['Plain body', 'Round eyes', 'Straight brows', 'Smile']);
});

it('a preview never shows over a newer real change: it is tried again on the skin as it is now', async () => {
  const { user } = await openCreate({ pieces: [BODY, TEE], painting: false });
  await user.click(screen.getByRole('button', { name: 'Hats' }));
  await user.click(tile('Hats', /^Cap/));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'T-shirt', 'Cap']));
  await user.click(screen.getByRole('button', { name: 'Shoes' }));
  const withCap = stageSum();
  fireEvent.pointerEnter(tile('Shoes', /Moon boots/), { pointerType: 'mouse' });
  await waitFor(() => expect(badge()).not.toBeNull());
  const bootsOverCap = stageSum();
  // Undo takes the cap off under the preview.
  await user.keyboard('{Control>}z{/Control}');
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'T-shirt']));
  await waitFor(() => expect(stageSum()).not.toBe(bootsOverCap));
  await waitFor(() => expect(badge()).not.toBeNull());
  const bootsNoCap = stageSum();
  expect(bootsNoCap).not.toBe(withCap);
  // Wearing the boots now gives exactly that.
  await user.click(tile('Shoes', /Moon boots/));
  await waitFor(() => expect(chips()).toEqual(['Plain body', 'T-shirt', 'Moon boots']));
  await waitFor(() => expect(badge()).toBeNull());
  expect(stageSum()).toBe(bootsNoCap);
});

it('a version preview takes the stage from a piece being tried on', async () => {
  const { user } = await openCreate({ pieces: [BODY, TEE], painting: false });
  await user.click(screen.getByRole('button', { name: 'Hats' }));
  const real = stageSum();
  fireEvent.pointerEnter(tile('Hats', /^Cap/), { pointerType: 'mouse' });
  await waitFor(() => expect(badge()).not.toBeNull());
  await user.click(screen.getByRole('button', { name: 'History' }));
  expect(badge()).toBeNull();
  expect(stageSum()).toBe(real);
});
