import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { InstanceIcon } from '../../art/InstanceIcon';
import { createMockBackend } from '../../ipc/mock';
import type { InstanceListing } from '../../ipc/schema';
import { qk } from '../../state/queries';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { Rail } from './Rail';

it('lists instances, then damaged ones, and selects on click', async () => {
  const { user } = renderApp(<Rail />);
  const asalith = await screen.findByRole('button', { name: 'Asalith Fields' });
  expect(screen.getByRole('button', { name: 'Creative Flat' })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'old-pack, Needs attention' })).toBeInTheDocument();
  expect(asalith).toHaveAttribute('aria-current', 'page');
  await user.click(screen.getByRole('button', { name: 'Creative Flat' }));
  expect(routeSnapshot()).toEqual({ name: 'home', id: 'creative-flat' });
});

it('moves focus with the arrow keys and enters at the selected item', async () => {
  const { user } = renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  // The rail key comes first, at the top.
  await user.tab();
  expect(screen.getByRole('button', { name: 'Show names' })).toHaveFocus();
  await user.tab();
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).toHaveFocus();
  await user.keyboard('{ArrowDown}');
  expect(screen.getByRole('button', { name: 'Creative Flat' })).toHaveFocus();
  await user.keyboard('{End}');
  expect(screen.getByRole('button', { name: 'old-pack, Needs attention' })).toHaveFocus();
});

it('marks a running instance', async () => {
  renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(screen.getByRole('button', { name: 'Asalith Fields, Playing' })).toBeInTheDocument();
});

it('marks an instance that is waking its server as getting ready', async () => {
  renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'waking', exit: null, update: null }));
  expect(screen.getByRole('button', { name: 'Asalith Fields, Getting ready' })).toBeInTheDocument();
});

it("uses a pack's icon", async () => {
  renderApp(<Rail />);
  const sky = await screen.findByRole('button', { name: 'Sky Islands' });
  await vi.waitFor(() => expect(sky.querySelector('img')?.getAttribute('src')).toMatch(/^data:image\//));
  expect(screen.getByRole('button', { name: 'Creative Flat' }).querySelector('img')).toBeNull();
});

it('shows a monogram of the name on a cabinet without art, and keeps the name as its accessible name', async () => {
  renderApp(<Rail />);
  const creative = await screen.findByRole('button', { name: 'Creative Flat' });
  expect(creative.querySelector('img')).toBeNull();
  expect(within(creative).getAllByText('CF').length).toBeGreaterThan(0);
  expect(creative.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
});

it("falls back to the generated icon when a pack's icon fails to decode", async () => {
  renderApp(<Rail />);
  const sky = await screen.findByRole('button', { name: 'Sky Islands' });
  await vi.waitFor(() => expect(sky.querySelector('img')).not.toBeNull());
  fireEvent.error(sky.querySelector('img')!);
  expect(sky.querySelector('img')).toBeNull();
  expect(sky.querySelector('svg')).not.toBeNull();
});

it('opens Servers from the rail, above Browse, and marks it while open', async () => {
  const { user } = renderApp(<Rail />);
  const servers = await screen.findByRole('button', { name: 'Servers' });
  const browse = screen.getByRole('button', { name: 'Browse' });
  expect(servers.compareDocumentPosition(browse) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(servers).not.toHaveAttribute('aria-current');
  await user.click(servers);
  expect(routeSnapshot()).toEqual({ name: 'servers' });
  expect(servers).toHaveAttribute('aria-current', 'page');
  // Servers isn't an instance's screen, so no cabinet is selected.
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).not.toHaveAttribute('aria-current');
});

it('opens Browse from the rail', async () => {
  const { user } = renderApp(<Rail />);
  await user.click(await screen.findByRole('button', { name: 'Browse' }));
  expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'modpack' });
});

it('marks a server with its last status, in words too', async () => {
  const { container } = renderApp(<Rail />, { server: 'online' });
  expect(await screen.findByRole('button', { name: 'Survival, Online · 3/20' })).toBeInTheDocument();
  expect(container.querySelectorAll('.rail-server')).toHaveLength(1);
  expect(container.querySelector('.rail-server')).toHaveAttribute('data-status', 'online');
  expect(container.querySelector('.rail-led')).toBeNull();
});

it("shows a server's status word on hover too", async () => {
  renderApp(<Rail />, { server: 'starting' });
  const survival = await screen.findByRole('button', { name: 'Survival, Starting' });
  expect(survival).toHaveAttribute('title', 'Survival · Starting');
  expect(screen.getByRole('button', { name: 'Creative Flat' })).toHaveAttribute('title', 'Creative Flat');
});

it('shows no names in the compact rail', async () => {
  renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  expect(screen.getByRole('button', { name: 'Show names' })).toHaveAttribute('aria-pressed', 'false');
  expect(screen.queryByText('Asalith Fields')).toBeNull();
  expect(screen.queryByText('Creative Flat')).toBeNull();
});

it('shows names when its key is pressed, and remembers it in the settings', async () => {
  const { user, calls } = renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  await user.click(screen.getByRole('button', { name: 'Show names' }));
  expect(await screen.findByText('Asalith Fields')).toBeVisible();
  expect(screen.getByText('Creative Flat')).toBeVisible();
  expect(sent(calls, 'settings_set')).toEqual([{ settings: expect.objectContaining({ railExpanded: true }) }]);
  const hide = screen.getByRole('button', { name: 'Hide names' });
  expect(hide).toHaveAttribute('aria-pressed', 'true');
  // The rows keep their accessible names, so the rest of the app still finds them.
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).toBeInTheDocument();
  await user.click(hide);
  await vi.waitFor(() => expect(screen.queryByText('Asalith Fields')).toBeNull());
  expect(sent(calls, 'settings_set').at(-1)).toEqual({ settings: expect.objectContaining({ railExpanded: false }) });
});

it('round icon shape marks the rail', async () => {
  const { container } = renderApp(<Rail />, { iconShape: 'round' });
  await screen.findByRole('button', { name: 'Asalith Fields' });
  expect(container.querySelector('nav.rail')).toHaveAttribute('data-icon-shape', 'round');
});

it('leaves the rail square by default', async () => {
  const { container } = renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  expect(container.querySelector('nav.rail')).toHaveAttribute('data-icon-shape', 'square');
});

it('shows the full name on hover and the status word in a wide row', async () => {
  renderApp(<Rail />, { railExpanded: true, server: 'starting' });
  expect(await screen.findByText('Asalith Fields')).toHaveAttribute('title', 'Asalith Fields');
  expect(await screen.findByText('Starting')).toBeVisible();
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(screen.getByRole('button', { name: 'Asalith Fields, Playing' })).toHaveTextContent('Playing');
});

it('moves focus with the arrow keys in the wide rail too', async () => {
  const { user } = renderApp(<Rail />, { railExpanded: true });
  await screen.findByText('Asalith Fields');
  await user.tab();
  await user.tab();
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).toHaveFocus();
  await user.keyboard('{ArrowDown}');
  expect(screen.getByRole('button', { name: 'Creative Flat' })).toHaveFocus();
  await user.keyboard('{ArrowUp}');
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).toHaveFocus();
});

it('moves a cabinet down with Alt+ArrowDown, keeps focus on it and says where it went', async () => {
  const { user, calls } = renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  await user.tab();
  await user.tab();
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).toHaveFocus();
  await user.keyboard('{Alt>}{ArrowDown}{/Alt}');
  // Only healthy instances are sent; the damaged one stays last on its own.
  expect(sent(calls, 'instance_reorder')).toEqual([{ ids: ['creative-flat', 'asalith-fields', 'sky-islands'] }]);
  await vi.waitFor(() => expect(cabinetNames()).toEqual(['Creative Flat', 'Asalith Fields', 'Sky Islands', 'old-pack, Needs attention']));
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).toHaveFocus();
  expect(screen.getByText('Asalith Fields moved to position 2 of 3')).toHaveAttribute('aria-live', 'polite');
  // It was on screen before the move (the first instance, nothing chosen yet), and it stays there.
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).toHaveAttribute('aria-current', 'page');
});

it('moves a cabinet up with Alt+ArrowUp in the wide rail', async () => {
  const { user, calls } = renderApp(<Rail />, { railExpanded: true });
  await screen.findByText('Asalith Fields');
  act(() => screen.getByRole('button', { name: 'Sky Islands' }).focus());
  await user.keyboard('{Alt>}{ArrowUp}{/Alt}');
  expect(sent(calls, 'instance_reorder')).toEqual([{ ids: ['asalith-fields', 'sky-islands', 'creative-flat'] }]);
  await vi.waitFor(() => expect(cabinetNames().slice(0, 3)).toEqual(['Asalith Fields', 'Sky Islands', 'Creative Flat']));
  expect(screen.getByRole('button', { name: 'Sky Islands' })).toHaveFocus();
});

it('does nothing on Alt+ArrowUp at the top, and never moves past the end or a damaged cabinet', async () => {
  const { user, calls } = renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  await user.tab();
  await user.tab();
  await user.keyboard('{Alt>}{ArrowUp}{/Alt}');
  // Alt+arrow is a move, never a plain roving step (which would wrap to the last cabinet).
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).toHaveFocus();
  act(() => screen.getByRole('button', { name: 'Sky Islands' }).focus());
  await user.keyboard('{Alt>}{ArrowDown}{/Alt}');
  expect(screen.getByRole('button', { name: 'Sky Islands' })).toHaveFocus();
  act(() => screen.getByRole('button', { name: 'old-pack, Needs attention' }).focus());
  await user.keyboard('{Alt>}{ArrowUp}{/Alt}');
  expect(sent(calls, 'instance_reorder')).toEqual([]);
  expect(cabinetNames()).toEqual(['Asalith Fields', 'Creative Flat', 'Sky Islands', 'old-pack, Needs attention']);
});

it('puts the order back when the move is refused', async () => {
  const { user, backend, calls } = renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  const handle = backend.handle;
  backend.handle = (cmd, args) =>
    cmd === 'instance_reorder'
      ? Promise.reject({ category: 'disk', code: 'io', message: 'The disk said no', fix: null, detail: null, params: {} })
      : handle(cmd, args);
  act(() => screen.getByRole('button', { name: 'Asalith Fields' }).focus());
  await user.keyboard('{Alt>}{ArrowDown}{/Alt}');
  await vi.waitFor(() => expect(sent(calls, 'instance_reorder')).toHaveLength(1));
  await vi.waitFor(() => expect(cabinetNames().slice(0, 3)).toEqual(['Asalith Fields', 'Creative Flat', 'Sky Islands']));
});

/** Lay the cabinets out 56px tall with 12px gaps, the way the rail draws them (jsdom has no layout). */
function layOut() {
  screen.getAllByRole('listitem').forEach((li, index) => {
    const button = li.querySelector('button')!;
    const top = index * 68;
    button.getBoundingClientRect = () => ({ top, bottom: top + 56, height: 56, left: 0, right: 56, width: 56, x: 0, y: top, toJSON: () => null });
  });
}

it('drags a cabinet to a new place, showing where it will land, without selecting it', async () => {
  const { calls } = renderApp(<Rail />);
  const creative = await screen.findByRole('button', { name: 'Creative Flat' });
  layOut();
  fireEvent.pointerDown(creative, { pointerId: 1, button: 0, clientX: 28, clientY: 96 });
  fireEvent.pointerMove(creative, { pointerId: 1, clientX: 28, clientY: 60 });
  fireEvent.pointerMove(creative, { pointerId: 1, clientX: 28, clientY: 10 });
  expect(creative).toHaveAttribute('data-dragging');
  // Asalith snaps down into Creative's place, so the open slot (a ground socket with the drop line) is at the top, under the carried cabinet.
  expect(shiftOf('Asalith Fields')).toBe('68px');
  expect(shiftOf('Sky Islands')).toBe('');
  const held = creative.closest('li')!;
  expect(held).toHaveAttribute('data-carry');
  expect(held).toHaveAttribute('data-drop');
  expect(held.style.getPropertyValue('--socket')).toBe('-68px');
  fireEvent.pointerUp(creative, { pointerId: 1, clientX: 28, clientY: 10 });
  fireEvent.click(creative);
  await vi.waitFor(() => expect(sent(calls, 'instance_reorder')).toEqual([{ ids: ['creative-flat', 'asalith-fields', 'sky-islands'] }]));
  expect(routeSnapshot()).not.toEqual({ name: 'home', id: 'creative-flat' });
  expect(creative).not.toHaveAttribute('data-dragging');
  expect(document.querySelector('[data-drop], [data-carry], [data-shift]')).toBeNull();
  expect(await screen.findByText('Creative Flat moved to position 1 of 3')).toBeInTheDocument();
});

it('drops below the last healthy cabinet, never among the damaged ones', async () => {
  const { calls } = renderApp(<Rail />, { railExpanded: true });
  const asalith = await screen.findByRole('button', { name: 'Asalith Fields' });
  layOut();
  fireEvent.pointerDown(asalith, { pointerId: 1, button: 0, clientX: 28, clientY: 28 });
  fireEvent.pointerMove(asalith, { pointerId: 1, clientX: 28, clientY: 400 });
  // Creative and Sky snap up a place; the open slot is the last healthy place, never among the damaged ones.
  expect(shiftOf('Creative Flat')).toBe('-68px');
  expect(shiftOf('Sky Islands')).toBe('-68px');
  expect(shiftOf('old-pack, Needs attention')).toBe('');
  expect(asalith.closest('li')!.style.getPropertyValue('--socket')).toBe('136px');
  fireEvent.pointerUp(asalith, { pointerId: 1, clientX: 28, clientY: 400 });
  await vi.waitFor(() => expect(sent(calls, 'instance_reorder')).toEqual([{ ids: ['creative-flat', 'sky-islands', 'asalith-fields'] }]));
});

it('still selects on a click that wobbles under 4px, and never drags a damaged cabinet', async () => {
  const { calls } = renderApp(<Rail />);
  const creative = await screen.findByRole('button', { name: 'Creative Flat' });
  layOut();
  fireEvent.pointerDown(creative, { pointerId: 1, button: 0, clientX: 28, clientY: 96 });
  fireEvent.pointerMove(creative, { pointerId: 1, clientX: 30, clientY: 98 });
  expect(creative).not.toHaveAttribute('data-dragging');
  fireEvent.pointerUp(creative, { pointerId: 1, clientX: 30, clientY: 98 });
  fireEvent.click(creative);
  expect(routeSnapshot()).toEqual({ name: 'home', id: 'creative-flat' });
  const damaged = screen.getByRole('button', { name: 'old-pack, Needs attention' });
  fireEvent.pointerDown(damaged, { pointerId: 2, button: 0, clientX: 28, clientY: 232 });
  fireEvent.pointerMove(damaged, { pointerId: 2, clientX: 28, clientY: 0 });
  expect(damaged).not.toHaveAttribute('data-dragging');
  fireEvent.pointerUp(damaged, { pointerId: 2, clientX: 28, clientY: 0 });
  expect(sent(calls, 'instance_reorder')).toEqual([]);
});

/** The cabinets' accessible names, top to bottom. */
function cabinetNames() {
  return screen
    .getAllByRole('listitem')
    .map((li) => li.querySelector('button')?.getAttribute('aria-label'))
    .filter(Boolean);
}

/** How far a row has snapped to make room for the carried cabinet ('' when it hasn't moved). */
function shiftOf(name: string) {
  return screen.getByRole('button', { name }).closest('li')!.style.getPropertyValue('--shift');
}

it('makes no room before the pointer has picked the cabinet up, and none once it is back home', async () => {
  renderApp(<Rail />);
  const creative = await screen.findByRole('button', { name: 'Creative Flat' });
  layOut();
  fireEvent.pointerDown(creative, { pointerId: 1, button: 0, clientX: 28, clientY: 96 });
  fireEvent.pointerMove(creative, { pointerId: 1, clientX: 28, clientY: 110 });
  // Over its own place: held, its socket under it, and nothing else moves or marks a drop.
  expect(creative.closest('li')).toHaveAttribute('data-carry');
  expect(creative.closest('li')!.style.getPropertyValue('--socket')).toBe('0px');
  expect(document.querySelector('[data-drop], [data-shift]')).toBeNull();
  fireEvent.pointerUp(creative, { pointerId: 1, clientX: 28, clientY: 110 });
});

it('puts the cabinet back on Escape, without the Escape reaching anything else', async () => {
  const { calls } = renderApp(<Rail />);
  const creative = await screen.findByRole('button', { name: 'Creative Flat' });
  layOut();
  const outside = vi.fn();
  document.addEventListener('keydown', outside);
  fireEvent.pointerDown(creative, { pointerId: 1, button: 0, clientX: 28, clientY: 96 });
  fireEvent.pointerMove(creative, { pointerId: 1, clientX: 28, clientY: 10 });
  fireEvent.keyDown(creative, { key: 'Escape' });
  document.removeEventListener('keydown', outside);
  expect(outside).not.toHaveBeenCalled();
  expect(creative).not.toHaveAttribute('data-dragging');
  expect(document.querySelector('[data-drop], [data-carry], [data-shift]')).toBeNull();
  fireEvent.pointerUp(creative, { pointerId: 1, clientX: 28, clientY: 10 });
  fireEvent.click(creative);
  expect(sent(calls, 'instance_reorder')).toEqual([]);
  expect(routeSnapshot()).not.toEqual({ name: 'home', id: 'creative-flat' });
});

it('drops nothing when the pointer is cancelled mid-drag', async () => {
  const { calls } = renderApp(<Rail />);
  const creative = await screen.findByRole('button', { name: 'Creative Flat' });
  layOut();
  fireEvent.pointerDown(creative, { pointerId: 1, button: 0, clientX: 28, clientY: 96 });
  fireEvent.pointerMove(creative, { pointerId: 1, clientX: 28, clientY: 10 });
  fireEvent.pointerCancel(creative, { pointerId: 1 });
  expect(creative).not.toHaveAttribute('data-dragging');
  expect(document.querySelector('[data-drop], [data-carry], [data-shift]')).toBeNull();
  fireEvent.pointerUp(creative, { pointerId: 1, clientX: 28, clientY: 10 });
  expect(sent(calls, 'instance_reorder')).toEqual([]);
});

it('moves the cabinet it carries even when the list changes mid-drag, and nothing once it is gone', async () => {
  const { calls, qc } = renderApp(<Rail />);
  const creative = await screen.findByRole('button', { name: 'Creative Flat' });
  layOut();
  const listing = qc.getQueryData<InstanceListing>(qk.instances)!;
  fireEvent.pointerDown(creative, { pointerId: 1, button: 0, clientX: 28, clientY: 96 });
  fireEvent.pointerMove(creative, { pointerId: 1, clientX: 28, clientY: 10 });
  // Another window deleted Asalith: Creative is first now, so a drop at the top leaves it there.
  act(() => qc.setQueryData<InstanceListing>(qk.instances, { ...listing, instances: listing.instances.filter((i) => i.id !== 'asalith-fields') }));
  await settle();
  fireEvent.pointerUp(creative, { pointerId: 1, clientX: 28, clientY: 10 });
  await settle();
  expect(sent(calls, 'instance_reorder')).toEqual([]);

  const sky = screen.getByRole('button', { name: 'Sky Islands' });
  layOut();
  fireEvent.pointerDown(sky, { pointerId: 2, button: 0, clientX: 28, clientY: 96 });
  fireEvent.pointerMove(sky, { pointerId: 2, clientX: 28, clientY: 10 });
  act(() => qc.setQueryData<InstanceListing>(qk.instances, { ...listing, instances: listing.instances.filter((i) => i.id !== 'sky-islands') }));
  await settle();
  fireEvent.pointerUp(sky, { pointerId: 2, clientX: 28, clientY: 10 });
  await settle();
  expect(sent(calls, 'instance_reorder')).toEqual([]);
});

it('hands focus back after a move only once, not on every later change to the list', async () => {
  const { user, qc } = renderApp(<Rail />);
  const asalith = await screen.findByRole('button', { name: 'Asalith Fields' });
  act(() => asalith.focus());
  await user.keyboard('{Alt>}{ArrowDown}{/Alt}');
  await vi.waitFor(() => expect(cabinetNames()[1]).toBe('Asalith Fields'));
  await vi.waitFor(() => expect(screen.getByRole('button', { name: 'Asalith Fields' })).toHaveFocus());
  // The player clicks away (onto the marquee, say), then a run ends and the list refetches.
  act(() => (document.activeElement as HTMLElement).blur());
  const listing = qc.getQueryData<InstanceListing>(qk.instances)!;
  // (A real change: the query keeps an equal listing as the same object.)
  act(() =>
    qc.setQueryData<InstanceListing>(qk.instances, { ...listing, instances: listing.instances.map((i) => (i.id === 'sky-islands' ? { ...i, name: 'Sky Isles' } : i)) }),
  );
  await settle();
  expect(screen.getByRole('button', { name: 'Sky Isles' })).toBeInTheDocument();
  expect(document.activeElement).toBe(document.body);
});

it('grows each generated icon from the art seed, not the id', async () => {
  const backend = createMockBackend();
  backend.state.listing.instances = backend.state.listing.instances.map((i) =>
    i.id === 'creative-flat' ? { ...i, artSeed: 'old-name', art: 'generated' as const } : i,
  );
  renderWithProviders(<Rail />, { ipc: (cmd, args) => backend.handle(cmd, args) });
  const creative = await screen.findByRole('button', { name: 'Creative Flat' });
  const want = render(<InstanceIcon name="Creative Flat" seed="old-name" size={40} />).container.innerHTML;
  const not = render(<InstanceIcon name="Creative Flat" seed="creative-flat" size={40} />).container.innerHTML;
  expect(want).not.toBe(not);
  expect(creative.querySelector('svg')!.outerHTML).toBe(want);
});

/** Let a mutation that may have started reach the backend (it awaits before sending). */
async function settle() {
  await act(() => new Promise((resolve) => setTimeout(resolve, 20)));
}

it('never pulls focus back after a move that went nowhere', async () => {
  const { user, qc } = renderApp(<Rail />);
  const asalith = await screen.findByRole('button', { name: 'Asalith Fields' });
  act(() => asalith.focus());
  // Alt+Up on the first cabinet: no move, so no list change hands focus back and clears the follow.
  await user.keyboard('{Alt>}{ArrowUp}{/Alt}');
  act(() => (document.activeElement as HTMLElement).blur());
  const listing = qc.getQueryData<InstanceListing>(qk.instances)!;
  act(() =>
    qc.setQueryData<InstanceListing>(qk.instances, { ...listing, instances: listing.instances.map((i) => (i.id === 'sky-islands' ? { ...i, name: 'Sky Isles' } : i)) }),
  );
  await settle();
  expect(screen.getByRole('button', { name: 'Sky Isles' })).toBeInTheDocument();
  expect(document.activeElement).toBe(document.body);
});
