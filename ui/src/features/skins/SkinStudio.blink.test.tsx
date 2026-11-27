// The Skins screen hands the stage a closed-eyes frame only while a blink is honest: never over a version preview or
// a piece being tried on, never while a stroke is painted, and not on the Catalog tab.
import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import { renderWithProviders } from '../../test/render';
import { checksum } from '../../test/skinFakes';
import { Pixels } from './paint';
import SkinStudio from './SkinStudio';

vi.mock('./codec', async () => (await import('../../test/skinFakes')).fakeCodec());
vi.mock('./Stage', async () => ({ Stage: (await import('../../test/skinFakes')).FakeStage }));
// The frame itself is worked out in useBlink (tested there): here it is always the same one while it is asked for.
const blink = vi.hoisted(() => ({ frame: null as unknown, asked: [] as { skin: string | null; enabled: boolean }[] }));
vi.mock('./useBlink', async (original) => ({
  ...(await original<typeof import('./useBlink')>()),
  useBlinkFrame: (skin: string | null, _doc: unknown, _catalog: unknown, _model: unknown, enabled: boolean) => {
    blink.asked.push({ skin, enabled });
    return enabled ? blink.frame : null;
  },
}));

const SHUT = new Pixels();
SHUT.data.fill(90);
const ID = 'skin-0a1b2c3d4e5f';

beforeEach(() => {
  blink.frame = SHUT;
  blink.asked = [];
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({
    x: 0, y: 0, left: 0, top: 0, right: 256, bottom: 256, width: 256, height: 256, toJSON: () => ({}),
  } as DOMRect);
});

const handed = () => screen.getByTestId('stage').getAttribute('data-blink');
const texel = (x: number, y: number) => ({ clientX: x * 4 + 2, clientY: y * 4 + 2, button: 0, pointerId: 1 });

async function open() {
  const backend = createMockBackend();
  backend.state.skins.entries.find((e) => e.id === ID)!.look = { pieces: [{ id: 'body-plain', colours: {} }, { id: 'eyes-round', colours: {} }], painting: false };
  const result = renderWithProviders(<SkinStudio />, { ipc: (cmd, args) => backend.handle(cmd, args) });
  await screen.findByText('Saved');
  return result;
}

it('the stage gets the open skin’s closed-eyes frame on Paint and Create', async () => {
  const { user } = await open();
  expect(handed()).toBe(checksum(SHUT.data));
  expect(blink.asked.at(-1)).toEqual({ skin: ID, enabled: true });
  await user.click(screen.getByRole('tab', { name: 'Create' }));
  await screen.findByRole('group', { name: 'Categories' });
  expect(handed()).toBe(checksum(SHUT.data));
  await user.click(screen.getByRole('tab', { name: 'Catalog' }));
  expect(blink.asked.at(-1)!.enabled).toBe(false);
});

it('no blink while a stroke is being painted', async () => {
  await open();
  const overlay = within(screen.getByTestId('texture-panel')).getByRole('img');
  fireEvent.pointerDown(overlay, texel(20, 20));
  expect(handed()).toBe('');
  fireEvent.pointerMove(overlay, texel(21, 20));
  expect(handed()).toBe('');
  fireEvent.pointerUp(overlay, texel(21, 20));
  expect(handed()).toBe(checksum(SHUT.data));
});

it('no blink over a piece being tried on, or over a version preview', async () => {
  const { user } = await open();
  await user.click(screen.getByRole('tab', { name: 'Create' }));
  await user.click(await screen.findByRole('button', { name: 'Hats' }));
  const cap = within(screen.getByRole('list', { name: 'Hats' })).getByRole('button', { name: /^Cap/ });
  fireEvent.pointerEnter(cap, { pointerType: 'mouse' });
  await screen.findByTestId('stage-try');
  expect(handed()).toBe('');
  fireEvent.pointerLeave(cap);
  await waitFor(() => expect(screen.queryByTestId('stage-try')).toBeNull());
  expect(handed()).toBe(checksum(SHUT.data));
  await user.click(screen.getByRole('button', { name: 'History' }));
  const versions = await screen.findByRole('list', { name: 'Earlier versions' });
  await user.click(within(versions).getAllByRole('button')[1]!);
  await screen.findByRole('button', { name: 'Restore this version' });
  expect(handed()).toBe('');
});
