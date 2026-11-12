import { QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { toastsSnapshot } from '../../state/toasts';
import { qk } from '../../state/queries';
import { renderApp, sent } from '../../test/backend';
import { createMockBackend, MOCK_MICROSOFT_ACCOUNT } from '../../ipc/mock';
import { renderWithProviders } from '../../test/render';
import { blankSkin, starterSkin } from './starter';
import SkinStudio from './SkinStudio';

// jsdom has no canvas or WebGL: the codec hands back the starter skin, and the 3D stage is unavailable (the screen
// keeps working on the flat texture, as it would without WebGL).
// A test that needs to tell saved textures apart turns `tagged` on: each texture then encodes to a digest of its texels.
const encoding = vi.hoisted(() => ({
  tagged: false,
  digest: (data: Uint8ClampedArray) => `data:image/png;base64,DIGEST-${data.reduce((h, v, i) => (h * 31 + v * (i + 1)) % 1_000_000_007, 7)}`,
}));
vi.mock('./codec', () => ({
  // A kept version decodes to the blank figure, so restoring one changes texels.
  decodeTexture: (url: string) => Promise.resolve(url.includes('VERSION') ? blankSkin().data : starterSkin().data),
  encodeTexture: (data: Uint8ClampedArray) => (encoding.tagged ? encoding.digest(data) : 'data:image/png;base64,ENCODED'),
}));
vi.mock('./stageScene', () => ({
  createStage: () => {
    throw new Error('no WebGL');
  },
}));

beforeEach(() => {
  // The texture panel maps the pointer through its box: make it 256 px square, 4 px a texel.
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({
    x: 0, y: 0, left: 0, top: 0, right: 256, bottom: 256, width: 256, height: 256, toJSON: () => ({}),
  } as DOMRect);
});
afterEach(() => {
  vi.useRealTimers();
  encoding.tagged = false;
});

const STEVE = '5627dd98-e6be-3c21-b8a8-e92344183641';
const ALEX = MOCK_MICROSOFT_ACCOUNT.uuid;

const texel = (x: number, y: number) => ({ clientX: x * 4 + 2, clientY: y * 4 + 2, button: 0, pointerId: 1 });

async function paintTexel(x: number, y: number) {
  const overlay = within(screen.getByTestId('texture-panel')).getByRole('img');
  fireEvent.pointerDown(overlay, texel(x, y));
  fireEvent.pointerUp(overlay, texel(x, y));
}

it('opens the newest skin and lists the wardrobe', async () => {
  renderApp(<SkinStudio />);
  expect(await screen.findByRole('heading', { name: 'Skins' })).toBeInTheDocument();
  const list = await screen.findByRole('list', { name: 'Wardrobe' });
  expect(within(list).getByText('Beach day')).toBeInTheDocument();
  expect(within(list).getByText('Miner')).toBeInTheDocument();
  // Beach day is slim, so the arms key says Slim arms.
  expect(await screen.findByRole('button', { name: 'Slim arms' })).toHaveAttribute('title', 'Switch to classic arms');
});

it('hotbar keys choose tools, toggle mirror, and undo and redo strokes', async () => {
  const { user } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  await user.keyboard('3');
  expect(screen.getByRole('radio', { name: 'Fill' })).toHaveAttribute('aria-checked', 'true');
  await user.keyboard('1');
  expect(screen.getByRole('radio', { name: 'Pencil' })).toHaveAttribute('aria-checked', 'true');
  await user.keyboard('7');
  expect(screen.getByRole('button', { name: 'Mirror' })).toHaveAttribute('aria-pressed', 'true');

  const undo = screen.getByRole('button', { name: 'Undo' });
  expect(undo).toBeDisabled();
  await waitFor(() => expect(screen.getByTestId('texture-panel')).toBeInTheDocument());
  await act(() => paintTexel(10, 10));
  expect(undo).toBeEnabled();
  await user.keyboard('{Control>}z{/Control}');
  expect(undo).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Redo' })).toBeEnabled();
  await user.keyboard('{Control>}y{/Control}');
  expect(undo).toBeEnabled();
});

it('autosaves once shortly after a stroke', async () => {
  const { calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  await waitFor(() => expect(sent(calls, 'skin_texture').length).toBeGreaterThan(0));
  vi.useFakeTimers({ shouldAdvanceTime: true });
  await act(() => paintTexel(10, 10));
  await act(() => paintTexel(11, 10));
  expect(screen.getByText('Saving…')).toBeInTheDocument();
  await act(() => vi.advanceTimersByTimeAsync(700));
  expect(sent(calls, 'skin_save')).toEqual([{ id: 'skin-0a1b2c3d4e5f', texture: 'data:image/png;base64,ENCODED' }]);
  await waitFor(() => expect(screen.getByText('Saved')).toBeInTheDocument());
});

it('New → The Perlander adds a skin and opens it', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByRole('list', { name: 'Wardrobe' });
  await user.click(screen.getByRole('button', { name: 'New' }));
  await user.click(await screen.findByRole('menuitem', { name: 'The Perlander' }));
  await waitFor(() => expect(sent(calls, 'skin_create')).toHaveLength(1));
  // Chosen by the player, so it's theirs to keep: not a seeded starter.
  expect(sent(calls, 'skin_create')[0]).toMatchObject({ name: 'The Perlander', model: 'classic', seeded: false });
  expect(await screen.findByRole('button', { name: /The Perlander/, current: true })).toBeInTheDocument();
});

it('delete asks first', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  const list = await screen.findByRole('list', { name: 'Wardrobe' });
  await user.click(within(list).getByRole('button', { name: 'More for Miner' }));
  await user.click(await screen.findByRole('menuitem', { name: 'Delete…' }));
  expect(sent(calls, 'skin_delete')).toEqual([]);
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Delete' }));
  await waitFor(() => expect(sent(calls, 'skin_delete')).toEqual([{ id: 'skin-ffeeddccbbaa' }]));
  await waitFor(() => expect(within(list).queryByText('Miner')).not.toBeInTheDocument());
});

it('an offline account gets the explanation and no Wear key', async () => {
  renderApp(<SkinStudio />);
  expect(await screen.findByText(/Offline accounts can't change their skin in Minecraft/)).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Wear this skin' })).not.toBeInTheDocument();
});

it('a Microsoft account wears the skin and gets the lamp', async () => {
  const { user, calls } = renderApp(<SkinStudio />, { microsoft: 'signedIn' });
  await screen.findByRole('button', { name: /Alex's skin/, current: true });
  await user.click(screen.getByRole('button', { name: 'New' }));
  await user.click(await screen.findByRole('menuitem', { name: 'The Perlander' }));
  await screen.findByRole('button', { name: /The Perlander/, current: true });
  await user.click(screen.getByRole('button', { name: 'Wear this skin' }));
  await waitFor(() => expect(sent(calls, 'skin_wear')).toHaveLength(1));
  expect(sent(calls, 'skin_wear')[0]).toMatchObject({ uuid: ALEX });
  await waitFor(() => expect(toastsSnapshot().map((t) => t.notice?.title)).toContain("You're wearing The Perlander"));
  const list = screen.getByRole('list', { name: 'Wardrobe' });
  expect(within(list).getAllByText('Wearing')).toHaveLength(1);
  expect(within(within(list).getAllByRole('listitem')[0]!).getByText('The Perlander')).toBeInTheDocument();
});

it('a refused upload shows the translated error', async () => {
  const { user } = renderApp(<SkinStudio />, { microsoft: 'signedIn', skins: 'rejected' });
  await user.click(await screen.findByRole('button', { name: 'Wear this skin' }));
  await waitFor(() => expect(toastsSnapshot().map((t) => (t.error as { code?: string } | undefined)?.code)).toContain('skin_rejected'));
});

it('import runs once at a time: Import waits while a skin is being imported', async () => {
  const { user, calls, backend } = renderApp(<SkinStudio />);
  await screen.findByRole('list', { name: 'Wardrobe' });
  // Hold the import until the test lets it go.
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation(async (cmd, args) => {
    if (cmd === 'skin_import') await held;
    return handle(cmd, args);
  });
  const importKey = screen.getByRole('button', { name: 'Import' });
  await user.click(importKey);
  await waitFor(() => expect(importKey).toHaveAttribute('aria-disabled', 'true'));
  await user.click(importKey);
  await user.click(importKey);
  // One file picker and one import, however many presses.
  expect(sent(calls, 'plugin:dialog|open')).toHaveLength(1);
  release();
  await waitFor(() => expect(importKey).not.toHaveAttribute('aria-disabled'));
  expect(sent(calls, 'skin_import')).toHaveLength(1);
  expect(within(screen.getByRole('list', { name: 'Wardrobe' })).getAllByText('castle')).toHaveLength(1);
});

it('import adds the picked PNG to the wardrobe', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByRole('list', { name: 'Wardrobe' });
  await user.click(screen.getByRole('button', { name: 'Import' }));
  await waitFor(() => expect(sent(calls, 'skin_import')).toEqual([{ path: 'C:/Users/player/Pictures/castle.png', owner: STEVE }]));
  expect(await within(screen.getByRole('list', { name: 'Wardrobe' })).findByText('castle')).toBeInTheDocument();
});

it("an offline account's empty wardrobe starts with the Perlander", async () => {
  const { calls } = renderApp(<SkinStudio />, { skins: 'empty' });
  await waitFor(() => expect(sent(calls, 'skin_create')).toHaveLength(1));
  expect(sent(calls, 'skin_create')[0]).toMatchObject({ name: 'The Perlander', owner: STEVE, seeded: true });
});

it('a Microsoft account opens on the skin it wears, even with a Perlander made before signing in', async () => {
  const { calls, backend } = renderApp(<SkinStudio />, { microsoft: 'signedIn', skins: 'loose' });
  // The Perlander saved before starters were marked: its texels are the starter's, and nothing else marks it.
  backend.state.skins.textures.set('skin-00000000aaaa', 'data:image/png;base64,ENCODED');
  const current = await screen.findByRole('button', { name: /Alex's skin/, current: true });
  const list = screen.getByRole('list', { name: 'Wardrobe' });
  expect(within(list).getAllByRole('button', { current: true })).toEqual([current]);
  expect(within(list).getAllByRole('listitem')[0]).toContainElement(current);
  expect(sent(calls, 'skin_sync_current')).toEqual([{ uuid: ALEX, name: "Alex's skin" }]);
  // The old untouched Perlander saved with no account signed in is recognised by its texture and not carried over.
  expect(sent(calls, 'skins_list')).toContainEqual({ owner: ALEX, starter: 'data:image/png;base64,ENCODED' });
  expect(within(list).queryByText('The Perlander')).not.toBeInTheDocument();
  expect(within(list).getAllByRole('listitem')).toHaveLength(1);
  expect(sent(calls, 'skin_create')).toEqual([]);
  expect(within(current).getByText('Wearing')).toBeInTheDocument();
});

it("reopening the screen doesn't add the account's skin twice", async () => {
  const { backend, calls, qc, rerender } = renderApp(<SkinStudio key="first" />, { microsoft: 'signedIn' });
  await screen.findByRole('button', { name: /Alex's skin/, current: true });
  rerender(
    <QueryClientProvider client={qc}>
      <SkinStudio key="again" />
    </QueryClientProvider>,
  );
  await waitFor(() => expect(sent(calls, 'skin_sync_current')).toHaveLength(2));
  await screen.findByRole('button', { name: /Alex's skin/, current: true });
  expect(backend.state.skins.entries.filter((e) => e.owner === ALEX)).toHaveLength(1);
});

it('a Microsoft account wearing the default skin gets no made-up one', async () => {
  const { calls } = renderApp(<SkinStudio />, { microsoft: 'signedIn', skins: 'default' });
  expect(await screen.findByText(/wears Minecraft's default skin/)).toBeInTheDocument();
  expect(sent(calls, 'skin_create')).toEqual([]);
});

it('parts sit under the colours, and the Outer head switches the whole outer layer', async () => {
  const { user } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  expect(screen.queryByRole('button', { name: 'Parts' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Outer layer' })).not.toBeInTheDocument();
  const parts = screen.getByRole('region', { name: 'Parts' });
  const outer = within(parts).getByRole('button', { name: 'Outer on every part' });
  expect(outer).toHaveAttribute('aria-pressed', 'true');
  await user.click(outer);
  expect(outer).toHaveAttribute('aria-pressed', 'false');
  for (const box of within(parts).getAllByRole('checkbox', { name: /· Outer$/ })) expect(box).not.toBeChecked();
  for (const box of within(parts).getAllByRole('checkbox', { name: /· Skin$/ })) expect(box).toBeChecked();
  // One part back on lights the head again.
  await user.click(within(parts).getByRole('checkbox', { name: 'Head · Outer' }));
  expect(outer).toHaveAttribute('aria-pressed', 'true');
});

it('choosing a recent colour leaves the recent row in its order', async () => {
  const { user } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  const presets = screen.getAllByRole('button', { name: /^Colour #/ });
  await user.click(presets[0]!);
  await user.click(presets[1]!);
  const row = () => within(screen.getByRole('group', { name: 'Recent' })).getAllByRole('button').map((b) => b.title);
  const before = row();
  expect(before).toHaveLength(2);
  await user.click(within(screen.getByRole('group', { name: 'Recent' })).getAllByRole('button')[1]!);
  expect(row()).toEqual(before);
});

it('a colour being typed joins the recent row only once it settles', async () => {
  renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const hex = screen.getByRole('textbox', { name: 'Hex colour' });
  fireEvent.change(hex, { target: { value: '#112233' } });
  fireEvent.change(hex, { target: { value: '#445566' } });
  expect(screen.queryByRole('group', { name: 'Recent' })).not.toBeInTheDocument();
  await act(() => vi.advanceTimersByTimeAsync(600));
  const row = within(screen.getByRole('group', { name: 'Recent' })).getAllByRole('button');
  expect(row.map((b) => b.title)).toEqual(['#445566']);
});

it('the brush opacity starts full and can be turned down', async () => {
  renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  const slider = screen.getByRole('slider', { name: 'Opacity' });
  expect(slider).toHaveValue('100');
  fireEvent.change(slider, { target: { value: '40' } });
  expect(slider).toHaveAttribute('aria-valuetext', '40%');
});

it('the picker takes a see-through texel at its own opacity', async () => {
  const { user } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  const slider = screen.getByRole('slider', { name: 'Opacity' });
  // A texel of the empty outer layer painted at 40%…
  await user.click(within(screen.getByRole('radiogroup', { name: 'Paint on' })).getByRole('radio', { name: 'Outer' }));
  fireEvent.change(slider, { target: { value: '40' } });
  await paintTexel(41, 9);
  // …picked with full opacity set: the picker sets the slider back to 40%.
  fireEvent.change(slider, { target: { value: '100' } });
  await user.keyboard('4');
  await paintTexel(41, 9);
  expect(slider).toHaveValue('40');
  // Painted back over itself with the pencil and picked again, it is still 40%: the strokes don't pile up.
  await user.keyboard('1');
  await paintTexel(41, 9);
  await paintTexel(41, 9);
  await user.keyboard('4');
  fireEvent.change(slider, { target: { value: '100' } });
  await paintTexel(41, 9);
  expect(slider).toHaveValue('40');
});

/** The one arms key: it says the model the skin has, and a press switches to the other. */
const armsKey = () => screen.getByRole('button', { name: /^(Classic|Slim) arms$/ });

const wardrobeNames = () => within(screen.getByRole('list', { name: 'Wardrobe' })).getAllByRole('listitem').map((li) => within(li).getAllByText(/./)[0]!.textContent);

it('Alt+arrows move the focused skin in the wardrobe, and the order is saved', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  expect(wardrobeNames()).toEqual(['Beach day', 'Miner']);
  const list = screen.getByRole('list', { name: 'Wardrobe' });
  within(list).getByRole('button', { name: /^Miner/ }).focus();
  await user.keyboard('{Alt>}{ArrowUp}{/Alt}');
  await waitFor(() => expect(sent(calls, 'skin_reorder')).toEqual([{ owner: STEVE, ids: ['skin-ffeeddccbbaa', 'skin-0a1b2c3d4e5f'] }]));
  expect(wardrobeNames()).toEqual(['Miner', 'Beach day']);
  expect(screen.getByText('Miner moved to position 1 of 2')).toBeInTheDocument();
  // Focus stays on the skin that moved, and the open skin doesn't change.
  expect(within(list).getByRole('button', { name: /^Miner/ })).toHaveFocus();
  expect(screen.getByRole('heading', { level: 1 }).parentElement).toHaveTextContent('Beach day');
  // Already on top, it goes no further.
  await user.keyboard('{Alt>}{ArrowUp}{/Alt}');
  expect(sent(calls, 'skin_reorder')).toHaveLength(1);
});

it('a skin dragged in the wardrobe lands where it is dropped, without opening it', async () => {
  const { calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  const list = screen.getByRole('list', { name: 'Wardrobe' });
  const miner = within(list).getByRole('button', { name: /^Miner/ });
  // Every row measures the same in jsdom (midpoints at 128): above them is the top slot.
  fireEvent.pointerDown(miner, { button: 0, pointerId: 1, clientX: 50, clientY: 140 });
  fireEvent.pointerMove(miner, { pointerId: 1, clientX: 50, clientY: 60 });
  expect(miner.closest('li')).toHaveAttribute('data-carry');
  fireEvent.pointerUp(miner, { pointerId: 1, clientX: 50, clientY: 60 });
  fireEvent.click(miner);
  await waitFor(() => expect(sent(calls, 'skin_reorder')).toEqual([{ owner: STEVE, ids: ['skin-ffeeddccbbaa', 'skin-0a1b2c3d4e5f'] }]));
  expect(wardrobeNames()).toEqual(['Miner', 'Beach day']);
  expect(miner.closest('li')).not.toHaveAttribute('data-carry');
  // The press that ended the drag isn't a click: Beach day stays open.
  expect(miner).not.toHaveAttribute('aria-current');
});

it('a short press on a wardrobe skin still opens it', async () => {
  const { calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  const miner = within(screen.getByRole('list', { name: 'Wardrobe' })).getByRole('button', { name: /^Miner/ });
  fireEvent.pointerDown(miner, { button: 0, pointerId: 1, clientX: 50, clientY: 140 });
  fireEvent.pointerMove(miner, { pointerId: 1, clientX: 51, clientY: 141 });
  fireEvent.pointerUp(miner, { pointerId: 1, clientX: 51, clientY: 141 });
  fireEvent.click(miner);
  await waitFor(() => expect(miner).toHaveAttribute('aria-current', 'true'));
  expect(sent(calls, 'skin_reorder')).toHaveLength(0);
});

it('the skin menu moves a skin up or down', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  await user.click(screen.getByRole('button', { name: 'More for Beach day' }));
  // The top skin can't go up.
  expect(screen.queryByRole('menuitem', { name: 'Move up' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('menuitem', { name: 'Move down' }));
  await waitFor(() => expect(sent(calls, 'skin_reorder')).toEqual([{ owner: STEVE, ids: ['skin-ffeeddccbbaa', 'skin-0a1b2c3d4e5f'] }]));
  expect(wardrobeNames()).toEqual(['Miner', 'Beach day']);
});

it('a refused reorder puts the wardrobe back as it is', async () => {
  const backend = createMockBackend();
  const { user } = renderWithProviders(<SkinStudio />, {
    ipc: (cmd, args) =>
      cmd === 'skin_reorder'
        ? Promise.reject({ category: 'disk', code: 'disk_write_failed', message: "The skin list couldn't be saved", fix: null, detail: null, params: {} })
        : backend.handle(cmd, args),
  });
  await screen.findByText('Saved');
  within(screen.getByRole('list', { name: 'Wardrobe' })).getByRole('button', { name: /^Miner/ }).focus();
  await user.keyboard('{Alt>}{ArrowUp}{/Alt}');
  await waitFor(() => expect(toastsSnapshot().length).toBeGreaterThan(0));
  await waitFor(() => expect(wardrobeNames()).toEqual(['Beach day', 'Miner']));
});

it('switching to slim arms and straight back gives back the very same arms', async () => {
  encoding.tagged = true;
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  await waitFor(() => expect(sent(calls, 'skin_texture').length).toBeGreaterThan(0));
  const saved = () => sent(calls, 'skin_save').map((c) => (c as { texture: string }).texture);
  const original = encoding.digest(starterSkin().data);
  // Beach day's test texture is drawn for classic arms: first put its flag right.
  await user.click(armsKey());
  vi.useFakeTimers({ shouldAdvanceTime: true });
  await user.click(armsKey());
  await act(() => vi.advanceTimersByTimeAsync(700));
  await waitFor(() => expect(saved()).toHaveLength(1));
  expect(saved()[0]).not.toBe(original);
  await user.click(armsKey());
  await act(() => vi.advanceTimersByTimeAsync(700));
  await waitFor(() => expect(saved()).toHaveLength(2));
  expect(saved()[1]).toBe(original);
});

it('switching arms redraws them into the new layout, and one undo puts both back', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  await waitFor(() => expect(sent(calls, 'skin_texture').length).toBeGreaterThan(0));
  // Beach day is flagged slim, but the test texture is drawn for classic arms: going classic only fixes the flag.
  await user.click(armsKey());
  await waitFor(() => expect(sent(calls, 'skin_update')).toEqual([{ id: 'skin-0a1b2c3d4e5f', name: null, model: 'classic' }]));
  expect(armsKey()).toHaveTextContent('Classic arms');
  expect(screen.getByText('Saved')).toBeInTheDocument();
  // Back to slim, the classic arms are redrawn as slim ones, and saved.
  vi.useFakeTimers({ shouldAdvanceTime: true });
  await user.click(armsKey());
  expect(screen.getByText('Saving…')).toBeInTheDocument();
  await act(() => vi.advanceTimersByTimeAsync(700));
  await waitFor(() => expect(sent(calls, 'skin_save')).toHaveLength(1));
  // Undo returns to classic arms in one step.
  await user.keyboard('{Control>}z{/Control}');
  await waitFor(() => expect(sent(calls, 'skin_update').at(-1)).toMatchObject({ model: 'classic' }));
  expect(armsKey()).toHaveTextContent('Classic arms');
});

it('one arms key switches classic and slim, each press to the other', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  expect(armsKey()).toHaveTextContent('Slim arms');
  await user.click(armsKey());
  expect(armsKey()).toHaveTextContent('Classic arms');
  expect(armsKey()).toHaveAttribute('title', 'Switch to slim arms');
  await waitFor(() => expect(sent(calls, 'skin_update').at(-1)).toMatchObject({ model: 'classic' }));
  await user.click(armsKey());
  expect(armsKey()).toHaveTextContent('Slim arms');
  await waitFor(() => expect(sent(calls, 'skin_update').at(-1)).toMatchObject({ model: 'slim' }));
  expect(screen.queryByRole('radiogroup', { name: 'Arms' })).not.toBeInTheDocument();
});

it('the stage bar reads arms, then the pose, then Animation', async () => {
  renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  const bar = armsKey().parentElement!;
  const keys = within(bar).getAllByRole('button').map((b) => b.textContent?.trim());
  expect(keys).toEqual(['Slim arms', 'Standing', 'Animation']);
});

it("the system's reduced-motion setting doesn't turn Animation off: the switch is the player's", async () => {
  // jsdom has no matchMedia: stand one in that answers yes to reduced motion.
  const original = window.matchMedia;
  window.matchMedia = ((query: string) =>
    ({ matches: query.includes('prefers-reduced-motion'), media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList) as typeof window.matchMedia;
  try {
    const { user } = renderApp(<SkinStudio />);
    await screen.findByText('Saved');
    const animation = screen.getByRole('button', { name: 'Animation' });
    expect(animation).toHaveAttribute('aria-pressed', 'true');
    await user.click(animation);
    expect(animation).toHaveAttribute('aria-pressed', 'false');
  } finally {
    window.matchMedia = original;
  }
});

it('Animation is on until turned off, which stands the figure straight; picking a pose turns it back on', async () => {
  const { user } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  const animation = screen.getByRole('button', { name: 'Animation' });
  expect(animation).toHaveAttribute('aria-pressed', 'true');
  await user.click(animation);
  expect(animation).toHaveAttribute('aria-pressed', 'false');
  await user.click(animation);
  expect(animation).toHaveAttribute('aria-pressed', 'true');
  // Off, then a pose picked (even the one shown): the figure moves again.
  await user.click(animation);
  await user.click(screen.getByRole('button', { name: /^Pose/ }));
  await user.click(await screen.findByRole('menuitem', { name: 'Walking' }));
  expect(animation).toHaveAttribute('aria-pressed', 'true');
  expect(screen.queryByRole('button', { name: 'Pause' })).not.toBeInTheDocument();
});

it("switching accounts shows each account's own wardrobe", async () => {
  const { user, calls } = renderApp(<SkinStudio />, { microsoft: 'signedIn' });
  const wardrobe = () => screen.getByRole('list', { name: 'Wardrobe' });
  await screen.findByRole('button', { name: /Alex's skin/, current: true });
  expect(within(wardrobe()).queryByText('Beach day')).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Wardrobe of Alex' }));
  await user.click(await screen.findByRole('menuitem', { name: 'Steve' }));
  expect(await screen.findByRole('button', { name: /Beach day/, current: true })).toBeInTheDocument();
  expect(within(wardrobe()).queryByText("Alex's skin")).not.toBeInTheDocument();
  expect(sent(calls, 'skins_list')).toContainEqual({ owner: STEVE, starter: null });
  expect(screen.getByText(/Offline accounts can't change their skin in Minecraft/)).toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Wardrobe of Steve' }));
  await user.click(await screen.findByRole('menuitem', { name: 'Alex' }));
  expect(await screen.findByRole('button', { name: /Alex's skin/, current: true })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Wear this skin' })).toBeInTheDocument();
});

it("Edit skin on an account opens that account's wardrobe", async () => {
  renderApp(<SkinStudio account={STEVE} />, { microsoft: 'signedIn' });
  const list = await screen.findByRole('list', { name: 'Wardrobe' });
  expect(await within(list).findByText('Beach day')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Wardrobe of Steve' })).toBeInTheDocument();
});

async function openHistory(user: ReturnType<typeof renderApp>['user']) {
  await screen.findByText('Saved');
  await user.click(screen.getByRole('button', { name: 'History' }));
  return screen.findByRole('list', { name: 'Earlier versions' });
}

it('History lists the kept versions, newest first, and marks the current and worn ones', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  const list = await openHistory(user);
  expect(sent(calls, 'skin_versions')).toEqual([{ id: 'skin-0a1b2c3d4e5f' }]);
  const items = within(list).getAllByRole('button');
  expect(items).toHaveLength(4);
  expect(items[0]).toHaveTextContent('Now');
  expect(items[0]).toHaveTextContent('Restored');
  expect(items[1]).toHaveTextContent('Worn');
  expect(items[3]).toHaveTextContent('Exported');
});

it('picking a version previews it without painting, and Back to now leaves', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  const list = await openHistory(user);
  await user.click(within(list).getAllByRole('button')[2]!);
  // Its face loaded it once; the preview asks again.
  const asked = () => sent(calls, 'skin_version_texture').filter((a) => (a as { version: string }).version === '1762300000789');
  await waitFor(() => expect(asked()).toHaveLength(2));
  expect(await screen.findByRole('button', { name: 'Restore this version' })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Wear this skin' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Back to now' }));
  expect(screen.queryByRole('button', { name: 'Restore this version' })).not.toBeInTheDocument();
});

it('History shows a version kept by a save at once, not one asked for before it', async () => {
  const { user } = renderApp(<SkinStudio />);
  const wardrobe = await screen.findByRole('list', { name: 'Wardrobe' });
  await user.click(within(wardrobe).getByRole('button', { name: /^Miner/ }));
  await screen.findByText('Saved');
  await user.click(screen.getByRole('button', { name: 'History' }));
  expect(await screen.findByText('No earlier versions yet. One is kept as soon as you change this skin.')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Close history' }));
  // The first save of a skin with no history keeps the texture it overwrites.
  await paintTexel(9, 9);
  await screen.findByText('Saved', {}, { timeout: 3000 });
  await user.click(screen.getByRole('button', { name: 'History' }));
  const list = await screen.findByRole('list', { name: 'Earlier versions' });
  expect(within(list).getAllByRole('button')).toHaveLength(1);
});

it('History shows the version kept on leaving a changed skin when it comes back to it', async () => {
  encoding.tagged = true;
  const { user, calls } = renderApp(<SkinStudio />);
  const list = await openHistory(user);
  expect(within(list).getAllByRole('button')).toHaveLength(4);
  await user.click(screen.getByRole('button', { name: 'Close history' }));
  await paintTexel(9, 9);
  await waitFor(() => expect(sent(calls, 'skin_save')).toHaveLength(1), { timeout: 3000 });
  const wardrobe = screen.getByRole('list', { name: 'Wardrobe' });
  await user.click(within(wardrobe).getByRole('button', { name: /^Miner/ }));
  await waitFor(() => expect(sent(calls, 'skin_checkpoint')).toHaveLength(1));
  await user.click(within(wardrobe).getByRole('button', { name: /^Beach day/ }));
  await openHistory(user);
  await waitFor(() => expect(within(screen.getByRole('list', { name: 'Earlier versions' })).getAllByRole('button')).toHaveLength(5));
});

it('Restore makes the version the skin, and Undo takes it back', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  const list = await openHistory(user);
  await user.click(within(list).getAllByRole('button')[2]!);
  await user.click(await screen.findByRole('button', { name: 'Restore this version' }));
  await waitFor(() => expect(sent(calls, 'skin_restore')).toEqual([{ id: 'skin-0a1b2c3d4e5f', version: '1762300000789' }]));
  await waitFor(() => expect(toastsSnapshot().map((t) => t.notice?.title)).toContain('Version restored'));
  expect(screen.queryByRole('button', { name: 'Restore this version' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Undo' })).toBeEnabled();
  // The restored version is in the history already: it becomes Now, and no second copy of it is listed.
  await waitFor(() => expect(within(screen.getByRole('list', { name: 'Earlier versions' })).getAllByRole('button')[2]).toHaveTextContent('Now'));
  expect(within(screen.getByRole('list', { name: 'Earlier versions' })).getAllByRole('button')).toHaveLength(4);
});

it('Restore brings back the arms the version was drawn for, and Undo takes them back', async () => {
  // Beach day has slim arms now; its oldest version was drawn for classic ones.
  const { user, calls } = renderApp(<SkinStudio />);
  const list = await openHistory(user);
  expect(screen.getByRole('button', { name: 'Slim arms' })).toBeInTheDocument();
  await user.click(within(list).getAllByRole('button')[3]!);
  await user.click(await screen.findByRole('button', { name: 'Restore this version' }));
  await waitFor(() => expect(sent(calls, 'skin_restore')).toHaveLength(1));
  expect(await screen.findByRole('button', { name: 'Classic arms' })).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Undo' }));
  expect(await screen.findByRole('button', { name: 'Slim arms' })).toBeInTheDocument();
  await waitFor(() => expect(sent(calls, 'skin_update')).toContainEqual({ id: 'skin-0a1b2c3d4e5f', name: null, model: 'slim' }));
});

it('Clear history asks first, then forgets every version and leaves the skin as it is', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  const list = await openHistory(user);
  // Previewing a version: clearing ends the preview too.
  await user.click(within(list).getAllByRole('button')[2]!);
  await screen.findByRole('button', { name: 'Restore this version' });
  await user.click(screen.getByRole('button', { name: 'Clear history' }));
  const confirm = screen.getByRole('dialog', { name: 'Clear the history of Beach day?' });
  expect(confirm).toHaveTextContent("Every earlier version of this skin is deleted. The skin stays as it is now. This can't be undone.");
  // Cancel keeps it all.
  await user.click(within(confirm).getByRole('button', { name: 'Cancel' }));
  expect(sent(calls, 'skin_versions_clear')).toEqual([]);
  await user.click(screen.getByRole('button', { name: 'Clear history' }));
  await user.click(within(screen.getByRole('dialog', { name: 'Clear the history of Beach day?' })).getByRole('button', { name: 'Clear history' }));
  await waitFor(() => expect(sent(calls, 'skin_versions_clear')).toEqual([{ id: 'skin-0a1b2c3d4e5f' }]));
  expect(await screen.findByText('No earlier versions yet. One is kept as soon as you change this skin.')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Restore this version' })).not.toBeInTheDocument();
  // Nothing left to clear.
  expect(screen.queryByRole('button', { name: 'Clear history' })).not.toBeInTheDocument();
});

it('Make a copy turns a version into a new wardrobe skin', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  const list = await openHistory(user);
  await user.click(within(list).getAllByRole('button')[1]!);
  await user.click(await screen.findByRole('button', { name: 'Make a copy' }));
  await waitFor(() => expect(sent(calls, 'skin_version_copy')).toHaveLength(1));
  expect(sent(calls, 'skin_version_copy')[0]).toMatchObject({ id: 'skin-0a1b2c3d4e5f', version: '1762400000456' });
  const name = (sent(calls, 'skin_version_copy')[0] as { name: string }).name;
  expect(await screen.findByRole('button', { name: new RegExp(name.replace(/[()]/g, '.')), current: true })).toBeInTheDocument();
});

it('leaving a skin that was changed keeps a version of it', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  await waitFor(() => expect(screen.getByTestId('texture-panel')).toBeInTheDocument());
  await act(() => paintTexel(10, 10));
  await user.click(within(screen.getByRole('list', { name: 'Wardrobe' })).getByRole('button', { name: /^Miner/ }));
  await waitFor(() => expect(sent(calls, 'skin_checkpoint')).toEqual([{ id: 'skin-0a1b2c3d4e5f' }]));
  // Leaving one that wasn't changed keeps nothing.
  await user.click(within(screen.getByRole('list', { name: 'Wardrobe' })).getByRole('button', { name: /^Beach day/ }));
  await waitFor(() => expect(sent(calls, 'skin_texture').length).toBeGreaterThan(2));
  expect(sent(calls, 'skin_checkpoint')).toHaveLength(1);
});

it('tabs switch between Create, Paint and Catalog', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  const tabs = screen.getByRole('tablist', { name: 'Skins' });
  expect(within(tabs).getAllByRole('tab').map((t) => t.textContent)).toEqual(['Create', 'Paint', 'Catalog']);
  // Paint is today's editor.
  expect(screen.getByRole('tab', { name: 'Paint' })).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByTestId('texture-panel')).toBeInTheDocument();
  expect(screen.getByRole('toolbar', { name: 'Tools' })).toBeInTheDocument();

  await user.click(screen.getByRole('tab', { name: 'Catalog' }));
  expect(await screen.findByRole('list', { name: 'Looks' })).toBeInTheDocument();
  expect(screen.queryByTestId('texture-panel')).not.toBeInTheDocument();
  expect(screen.queryByRole('toolbar', { name: 'Tools' })).not.toBeInTheDocument();
  // The wardrobe stays.
  expect(screen.getByRole('list', { name: 'Wardrobe' })).toBeInTheDocument();

  await user.click(screen.getByRole('tab', { name: 'Create' }));
  expect(screen.getByRole('tab', { name: 'Create' })).toHaveAttribute('aria-selected', 'true');
  expect(screen.queryByRole('list', { name: 'Looks' })).not.toBeInTheDocument();

  // Adding a catalog look puts it in the shown account's wardrobe and opens it there; the player stays in the catalog.
  await user.click(screen.getByRole('tab', { name: 'Catalog' }));
  await user.click(within(await screen.findByRole('list', { name: 'Looks' })).getByRole('button', { name: 'Astronaut' }));
  await user.click(screen.getByRole('button', { name: 'Add to my wardrobe' }));
  const wardrobe = screen.getByRole('list', { name: 'Wardrobe' });
  expect(await within(wardrobe).findByRole('button', { name: /^Astronaut/, current: true })).toBeInTheDocument();
  expect(screen.getByRole('tab', { name: 'Catalog' })).toHaveAttribute('aria-selected', 'true');
  expect(await screen.findByText('In your wardrobe')).toBeInTheDocument();
  expect(sent(calls, 'skin_create').at(-1)).toMatchObject({ name: 'Astronaut', owner: STEVE });
  expect(sent(calls, 'skin_look_save')).toHaveLength(1);
  // Customize takes that same skin to Create.
  await user.click(screen.getByRole('button', { name: 'Customize' }));
  await waitFor(() => expect(screen.getByRole('tab', { name: 'Create' })).toHaveAttribute('aria-selected', 'true'));
  expect(within(screen.getByRole('list', { name: 'Wardrobe' })).getByRole('button', { name: /^Astronaut/, current: true })).toBeInTheDocument();
  expect(sent(calls, 'skin_create').filter((c) => (c as { name: string }).name === 'Astronaut')).toHaveLength(1);
});

it('Customize on a catalog look adds it and opens it in Create in one go', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  await user.click(screen.getByRole('tab', { name: 'Catalog' }));
  await user.click(within(await screen.findByRole('list', { name: 'Looks' })).getByRole('button', { name: 'Knight' }));
  await user.click(screen.getByRole('button', { name: 'Customize' }));
  await waitFor(() => expect(screen.getByRole('tab', { name: 'Create' })).toHaveAttribute('aria-selected', 'true'));
  expect(within(screen.getByRole('list', { name: 'Wardrobe' })).getByRole('button', { name: /^Knight/, current: true })).toBeInTheDocument();
  expect(sent(calls, 'skin_create').at(-1)).toMatchObject({ name: 'Knight', owner: STEVE });
  expect(sent(calls, 'skin_look_save')).toHaveLength(1);
});

it('unavailable without cache shows the message and Paint still works', async () => {
  const { user } = renderApp(<SkinStudio />, { skins: 'nocatalog' });
  await screen.findByText('Saved');
  await user.click(screen.getByRole('tab', { name: 'Catalog' }));
  expect(await screen.findByText("Perland's skin catalog can't be reached right now. Try again when you're online.")).toBeInTheDocument();
  await user.click(screen.getByRole('tab', { name: 'Paint' }));
  await waitFor(() => expect(screen.getByTestId('texture-panel')).toBeInTheDocument());
  const undo = screen.getByRole('button', { name: 'Undo' });
  await act(() => paintTexel(10, 10));
  expect(undo).toBeEnabled();
});

it("asks for the player's figure again on the way out, so Home and the Lobby show what was changed here", async () => {
  const { qc, unmount } = renderApp(<SkinStudio />);
  await screen.findByRole('heading', { name: 'Skins' });
  qc.setQueryData(qk.playerSkin('some-account'), { texture: null, model: 'classic', wardrobe: 0 });
  expect(qc.getQueryState(qk.playerSkin('some-account'))?.isInvalidated).toBe(false);
  unmount();
  expect(qc.getQueryState(qk.playerSkin('some-account'))?.isInvalidated).toBe(true);
});
