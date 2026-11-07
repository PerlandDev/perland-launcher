import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
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
  expect(sent(calls, 'skin_create')[0]).toMatchObject({ name: 'The Perlander', model: 'classic' });
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
  await user.click(await screen.findByRole('button', { name: 'Wear this skin' }));
  await waitFor(() => expect(sent(calls, 'skin_wear')).toHaveLength(1));
  expect(sent(calls, 'skin_wear')[0]).toMatchObject({ id: 'skin-0a1b2c3d4e5f' });
  await waitFor(() => expect(toastsSnapshot().map((t) => t.notice?.title)).toContain("You're wearing Beach day"));
  expect(within(screen.getByRole('list', { name: 'Wardrobe' })).getByText('Wearing')).toBeInTheDocument();
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
  await waitFor(() => expect(sent(calls, 'skin_import')).toEqual([{ path: 'C:/Users/player/Pictures/castle.png' }]));
  expect(await within(screen.getByRole('list', { name: 'Wardrobe' })).findByText('castle')).toBeInTheDocument();
});

it('an empty wardrobe starts with the Perlander', async () => {
  const { calls } = renderApp(<SkinStudio />, { skins: 'empty' });
  await waitFor(() => expect(sent(calls, 'skin_create')).toHaveLength(1));
  expect(sent(calls, 'skin_create')[0]).toMatchObject({ name: 'The Perlander' });
});

it("a Microsoft account's empty wardrobe starts from the skin it wears", async () => {
  const { backend, calls } = renderApp(<SkinStudio />, { microsoft: 'signedIn', skins: 'empty' });
  backend.state.skins.current = { texture: 'data:image/png;base64,REAL', model: 'slim' };
  await waitFor(() => expect(sent(calls, 'skin_create')).toHaveLength(1));
  expect(sent(calls, 'skin_create')[0]).toMatchObject({ model: 'slim', texture: 'data:image/png;base64,REAL' });
});

it('a Microsoft account wearing the default skin gets no made-up one', async () => {
  const { calls } = renderApp(<SkinStudio />, { microsoft: 'signedIn', skins: 'empty' });
  expect(await screen.findByText(/wears Minecraft's default skin/)).toBeInTheDocument();
  expect(sent(calls, 'skin_create')).toEqual([]);
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
