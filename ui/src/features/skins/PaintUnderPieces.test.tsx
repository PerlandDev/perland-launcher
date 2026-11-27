// Paint on a skin that wears catalog pieces: strokes go under the pieces. Uses the fake codec, whose pieces recolour.
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
const LOOK: SkinLook = {
  pieces: [
    { id: 'body-plain', colours: { skin: '#d9a27c' } },
    { id: 'top-tee', colours: { main: '#e8e4dc', trim: '#3a6ea5' } },
  ],
  painting: false,
};
const HINT = 'Pieces are on top. Take them off or make them part of the skin to paint over them.';

/** The Skins screen on its first skin wearing LOOK, on the Paint tab once the catalog has loaded. */
async function openPaint() {
  const backend = createMockBackend();
  backend.state.skins.entries.find((e) => e.id === ID)!.look = LOOK;
  const calls: [string, unknown][] = [];
  const result = renderWithProviders(<SkinStudio />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      return backend.handle(cmd, args);
    },
  });
  await screen.findByText('Saved');
  await result.user.click(screen.getByRole('tab', { name: 'Create' }));
  await screen.findByRole('group', { name: 'Categories' });
  await result.user.click(screen.getByRole('tab', { name: 'Paint' }));
  await screen.findByTestId('texture-panel');
  return { ...result, calls };
}

const texel = (x: number, y: number) => ({ clientX: x * 4 + 2, clientY: y * 4 + 2, button: 0, pointerId: 1 });
function paintTexel(x: number, y: number) {
  const overlay = within(screen.getByTestId('texture-panel')).getByRole('img');
  fireEvent.pointerDown(overlay, texel(x, y));
  fireEvent.pointerUp(overlay, texel(x, y));
}

it('a stroke hidden by a piece shows the pieces-on-top hint', async () => {
  await openPaint();
  expect(screen.queryByText(HINT)).not.toBeInTheDocument();
  // Under the T-shirt (the fake top covers rows 32–35); the pieces' files may still be loading at first.
  await waitFor(() => {
    act(() => paintTexel(10, 33));
    expect(screen.getByText(HINT)).toBeInTheDocument();
  });
  // A stroke that shows clears it.
  act(() => paintTexel(10, 40));
  expect(screen.queryByText(HINT)).not.toBeInTheDocument();
});

it('eyedropper picks the composed colour', async () => {
  const { user } = await openPaint();
  // Red goes into the painting under the shirt; the eyedropper picks what shows there: the shirt.
  const hex = screen.getByRole('textbox', { name: 'Hex colour' });
  fireEvent.change(hex, { target: { value: '#ff0000' } });
  fireEvent.blur(hex);
  await waitFor(() => {
    act(() => paintTexel(10, 33));
    expect(screen.getByText(HINT)).toBeInTheDocument();
  });
  await user.keyboard('4');
  expect(screen.getByRole('radio', { name: 'Picker' })).toHaveAttribute('aria-checked', 'true');
  act(() => paintTexel(10, 33));
  await waitFor(() => expect(screen.getByRole('textbox', { name: 'Hex colour' })).toHaveValue('#e8e4dc'));
});

it('restoring a version of a skin with pieces warns and clears the look', async () => {
  const { user, calls } = await openPaint();
  await user.click(screen.getByRole('button', { name: 'History' }));
  const list = await screen.findByRole('list', { name: 'Earlier versions' });
  await user.click(within(list).getAllByRole('button')[2]!);
  await user.click(await screen.findByRole('button', { name: 'Restore this version' }));
  const dialog = await screen.findByRole('dialog');
  expect(within(dialog).getByText(/becomes a plain painted skin/)).toBeInTheDocument();
  expect(sent(calls, 'skin_restore')).toEqual([]);
  await user.click(within(dialog).getByRole('button', { name: 'Restore' }));
  await waitFor(() => expect(sent(calls, 'skin_restore')).toHaveLength(1));
  await waitFor(() => expect(screen.queryByRole('button', { name: 'Restore this version' })).not.toBeInTheDocument());
  // The next save is a plain one: no look is saved back.
  const looks = sent(calls, 'skin_look_save').length;
  // The history panel holds the texture column: paint on the figure (the fake stage paints texel (10, 10)).
  act(() => void fireEvent.pointerDown(screen.getByTestId('stage')));
  await waitFor(() => expect(sent(calls, 'skin_save')).toHaveLength(1), { timeout: 3000 });
  expect(sent(calls, 'skin_look_save')).toHaveLength(looks);
  await user.click(screen.getByRole('tab', { name: 'Create' }));
  expect(within(screen.getByRole('list', { name: 'Wearing' })).queryAllByRole('listitem')).toHaveLength(0);
});
