import { act, fireEvent, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
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

it("uses a pack's icon", async () => {
  renderApp(<Rail />);
  const sky = await screen.findByRole('button', { name: 'Sky Islands' });
  await vi.waitFor(() => expect(sky.querySelector('img')?.getAttribute('src')).toMatch(/^data:image\//));
  expect(screen.getByRole('button', { name: 'Creative Flat' }).querySelector('img')).toBeNull();
});

it("falls back to the generated icon when a pack's icon fails to decode", async () => {
  renderApp(<Rail />);
  const sky = await screen.findByRole('button', { name: 'Sky Islands' });
  await vi.waitFor(() => expect(sky.querySelector('img')).not.toBeNull());
  fireEvent.error(sky.querySelector('img')!);
  expect(sky.querySelector('img')).toBeNull();
  expect(sky.querySelector('svg')).not.toBeNull();
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
