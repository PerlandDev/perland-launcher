import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import type { SkinLook } from '../../ipc/schema';
import { sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import SkinStudio from './SkinStudio';

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
async function openCreate(look?: SkinLook, { catalog = 'fresh' }: { catalog?: 'fresh' | 'unavailable' | 'never' } = {}) {
  const backend = createMockBackend();
  if (look) backend.state.skins.entries.find((e) => e.id === ID)!.look = look;
  if (catalog === 'unavailable') backend.state.skins.catalogStatus = 'unavailable';
  const calls: [string, unknown][] = [];
  const result = renderWithProviders(<SkinStudio />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      // 'never': the catalog is still loading for the whole test.
      if (cmd === 'skin_catalog' && catalog === 'never') return new Promise(() => {});
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
  await user.click(within(within(colours).getByRole('group', { name: 'Main' })).getAllByRole('button')[1]!);
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

it('build from pieces starts with the plain body', async () => {
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
  expect(save!.look).toEqual({ pieces: [{ id: 'body-plain', colours: { skin: '#d9a27c' } }], painting: false });
  expect(save!.painting).toBeNull();
  expect(save!.texture).toBe(created!.texture);
  // It opens in Create, wearing the body.
  expect(await screen.findByRole('tab', { name: 'Create', selected: true })).toBeInTheDocument();
  await waitFor(() => expect(chips()).toEqual(['Plain body']));
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
