import { QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { MOCK_MICROSOFT_ACCOUNT } from '../../ipc/mock';
import { blankSkin, starterSkin } from './starter';
import SkinStudio from './SkinStudio';

// jsdom has no canvas or WebGL: the codec hands back the starter skin, and the 3D stage is unavailable (the screen
// keeps working on the flat texture, as it would without WebGL).
vi.mock('./codec', () => ({
  // A kept version decodes to the blank figure, so restoring one changes texels.
  decodeTexture: (url: string) => Promise.resolve(url.includes('VERSION') ? blankSkin().data : starterSkin().data),
  encodeTexture: () => 'data:image/png;base64,ENCODED',
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
  // Beach day is slim, so Slim arms is the chosen model.
  expect(await screen.findByRole('radio', { name: 'Slim arms' })).toHaveAttribute('aria-checked', 'true');
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

it('switching arms redraws them into the new layout, and one undo puts both back', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  await waitFor(() => expect(sent(calls, 'skin_texture').length).toBeGreaterThan(0));
  // Beach day is flagged slim, but the test texture is drawn for classic arms: going classic only fixes the flag.
  await user.click(screen.getByRole('radio', { name: 'Classic arms' }));
  await waitFor(() => expect(sent(calls, 'skin_update')).toEqual([{ id: 'skin-0a1b2c3d4e5f', name: null, model: 'classic' }]));
  expect(screen.getByRole('radio', { name: 'Classic arms' })).toHaveAttribute('aria-checked', 'true');
  expect(screen.getByText('Saved')).toBeInTheDocument();
  // Back to slim, the classic arms are redrawn as slim ones, and saved.
  vi.useFakeTimers({ shouldAdvanceTime: true });
  await user.click(screen.getByRole('radio', { name: 'Slim arms' }));
  expect(screen.getByText('Saving…')).toBeInTheDocument();
  await act(() => vi.advanceTimersByTimeAsync(700));
  await waitFor(() => expect(sent(calls, 'skin_save')).toHaveLength(1));
  // Undo returns to classic arms in one step.
  await user.keyboard('{Control>}z{/Control}');
  await waitFor(() => expect(sent(calls, 'skin_update').at(-1)).toMatchObject({ model: 'classic' }));
  expect(screen.getByRole('radio', { name: 'Classic arms' })).toHaveAttribute('aria-checked', 'true');
});

it('the figure can be paused and resumed, and stays paused across poses', async () => {
  const { user } = renderApp(<SkinStudio />);
  await screen.findByText('Saved');
  await user.click(screen.getByRole('button', { name: 'Pause' }));
  expect(screen.getByRole('button', { name: 'Resume' })).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: /^Pose/ }));
  await user.click(await screen.findByRole('menuitem', { name: 'Walking' }));
  expect(screen.getByRole('button', { name: 'Resume' })).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Resume' }));
  expect(screen.getByRole('button', { name: 'Pause' })).toBeInTheDocument();
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

it('Restore makes the version the skin, and Undo takes it back', async () => {
  const { user, calls } = renderApp(<SkinStudio />);
  const list = await openHistory(user);
  await user.click(within(list).getAllByRole('button')[2]!);
  await user.click(await screen.findByRole('button', { name: 'Restore this version' }));
  await waitFor(() => expect(sent(calls, 'skin_restore')).toEqual([{ id: 'skin-0a1b2c3d4e5f', version: '1762300000789' }]));
  await waitFor(() => expect(toastsSnapshot().map((t) => t.notice?.title)).toContain('Version restored'));
  expect(screen.queryByRole('button', { name: 'Restore this version' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Undo' })).toBeEnabled();
  // The restore itself is the newest version now.
  await waitFor(() => expect(within(screen.getByRole('list', { name: 'Earlier versions' })).getAllByRole('button')).toHaveLength(5));
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
