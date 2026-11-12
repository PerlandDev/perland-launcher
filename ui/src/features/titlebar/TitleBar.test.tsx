import { act, screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { beginContentInstall } from '../../state/contentProgress';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { TitleBar } from './TitleBar';

it('drives the window and shows the active account', async () => {
  const { user, calls } = renderApp(<TitleBar />);
  expect(await screen.findByRole('button', { name: /Steve/ })).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Minimize' }));
  await user.click(screen.getByRole('button', { name: 'Maximize' }));
  expect(sent(calls, 'plugin:window|minimize')).toHaveLength(1);
  expect(sent(calls, 'plugin:window|toggle_maximize')).toHaveLength(1);
  await user.click(screen.getByRole('button', { name: 'Settings' }));
  expect(routeSnapshot()).toEqual({ name: 'settings' });
});

it('opens the Profile from the account, which sits pressed while it is open', async () => {
  const { user } = renderApp(<TitleBar />);
  const account = await screen.findByRole('button', { name: 'Steve, your profile' });
  expect(account).toHaveTextContent('Steve');
  expect(account).not.toHaveAttribute('aria-current');
  await user.click(account);
  expect(routeSnapshot()).toEqual({ name: 'profile' });
  expect(account).toHaveAttribute('aria-current', 'page');
  // No place is open then: the Profile is the account's, not a tab.
  for (const place of within(screen.getByRole('navigation', { name: 'Places' })).getAllByRole('button')) expect(place).not.toHaveAttribute('aria-current');
  await user.click(screen.getByRole('button', { name: 'Servers' }));
  expect(account).not.toHaveAttribute('aria-current');
});

it("names the launcher's places as tabs after the mark, before the account", async () => {
  renderApp(<TitleBar />);
  const account = await screen.findByRole('button', { name: /Steve/ });
  const places = within(screen.getByRole('navigation', { name: 'Places' })).getAllByRole('button');
  expect(places.map((p) => p.textContent)).toEqual(['Skins', 'Servers', 'Mods & more']);
  for (const place of places) expect(place.compareDocumentPosition(account) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  // The account is followed by Settings: Skins is a tab now, not an icon beside it.
  const buttons = screen.getAllByRole('button');
  expect(buttons.indexOf(screen.getByRole('button', { name: 'Settings' }))).toBe(buttons.indexOf(account) + 1);
});

it('opens each place and marks the one that is open', async () => {
  const { user } = renderApp(<TitleBar />);
  await screen.findByRole('button', { name: /Steve/ });
  // On an instance's Home (where the launcher starts) no place is open: the rail marks the instance.
  for (const name of ['Skins', 'Servers', 'Mods & more']) expect(screen.getByRole('button', { name })).not.toHaveAttribute('aria-current');
  await user.click(screen.getByRole('button', { name: 'Servers' }));
  expect(routeSnapshot()).toEqual({ name: 'servers' });
  expect(screen.getByRole('button', { name: 'Servers' })).toHaveAttribute('aria-current', 'page');
  await user.click(screen.getByRole('button', { name: 'Mods & more' }));
  expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'modpack' });
  await user.click(screen.getByRole('button', { name: 'Skins' }));
  expect(routeSnapshot()).toEqual({ name: 'skins' });
  expect(screen.getByRole('button', { name: 'Skins' })).toHaveAttribute('aria-current', 'page');
  expect(screen.getByRole('button', { name: 'Servers' })).not.toHaveAttribute('aria-current');
});

it('walks the places with the arrow keys from one tab stop', async () => {
  const { user } = renderApp(<TitleBar />);
  await screen.findByRole('button', { name: /Steve/ });
  await user.tab();
  expect(screen.getByRole('button', { name: 'Skins' })).toHaveFocus();
  await user.keyboard('{ArrowRight}');
  expect(screen.getByRole('button', { name: 'Servers' })).toHaveFocus();
  await user.keyboard('{End}');
  expect(screen.getByRole('button', { name: 'Mods & more' })).toHaveFocus();
  await user.keyboard('{ArrowLeft}{ArrowLeft}');
  expect(screen.getByRole('button', { name: 'Skins' })).toHaveFocus();
  await user.keyboard('{End}');
  // One stop for the places: the next Tab is the account.
  await user.tab();
  expect(screen.getByRole('button', { name: /Steve/ })).toHaveFocus();
});

it('has no places while nothing can be reached (first run, settings failed)', () => {
  renderApp(<TitleBar minimal />);
  expect(screen.queryByRole('navigation', { name: 'Places' })).toBeNull();
});

it('draws the settings button as a cog', async () => {
  renderApp(<TitleBar />);
  const button = await screen.findByRole('button', { name: 'Settings' });
  expect(button.querySelector('svg')).toHaveAttribute('data-icon', 'settings-cog');
});

it('keeps only the window controls when minimal', async () => {
  renderApp(<TitleBar minimal />, { firstRun: true });
  expect(await screen.findByRole('button', { name: 'Close' })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Minimize' })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Add account' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Settings' })).not.toBeInTheDocument();
});

it('asks for an account when there is none', async () => {
  renderApp(<TitleBar />, { firstRun: true });
  expect(await screen.findByRole('button', { name: 'Add account' })).toBeInTheDocument();
});

it('shows a quiet note with Restart now when an update is ready', async () => {
  const { user, calls } = renderApp(<TitleBar />, { beta: true, offlineLocked: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  expect(await screen.findByText('Update ready · installs when you close')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Restart now' }));
  expect(sent(calls, 'update_restart')).toHaveLength(1);
});

it('sends one restart for a double click', async () => {
  const { user, backend, calls } = renderApp(<TitleBar />, { beta: true, offlineLocked: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  // A real restart never answers: the launcher closes. Park it, so the second click lands while it's in flight.
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'update_restart' ? new Promise(() => {}) : handle(cmd, args)));
  await user.dblClick(await screen.findByRole('button', { name: 'Restart now' }));
  expect(sent(calls, 'update_restart')).toHaveLength(1);
});

it('disables Restart now while a game runs and says why in the note', async () => {
  renderApp(<TitleBar />, { beta: true, offlineLocked: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  const restart = await screen.findByRole('button', { name: 'Restart now' });
  expect(restart).toBeEnabled();
  expect(restart).not.toHaveAccessibleDescription();
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeDisabled();
  // Visible text, not a hover title: a disabled button can't be focused to show one.
  expect(screen.getByText('Update ready · wait for games and installs to finish')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Restart now' })).toHaveAccessibleDescription('Update ready · wait for games and installs to finish');
  expect(screen.getByRole('button', { name: 'Restart now' })).not.toHaveAttribute('title');
});

it('disables Restart now while a Browse install runs', async () => {
  renderApp(<TitleBar />, { beta: true, offlineLocked: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  expect(await screen.findByRole('button', { name: 'Restart now' })).toBeEnabled();
  let end = () => {};
  act(() => {
    end = beginContentInstall();
  });
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeDisabled();
  expect(screen.getByText('Update ready · wait for games and installs to finish')).toBeVisible();
  act(() => end());
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeEnabled();
});

it('shows no note when nothing is ready, and none in the minimal bar', async () => {
  const { unmount } = renderApp(<TitleBar />, { beta: true, offlineLocked: true, update: { state: 'up_to_date' } });
  await screen.findByRole('button', { name: 'Close' });
  expect(screen.queryByText('Update ready · installs when you close')).not.toBeInTheDocument();
  unmount();
  renderApp(<TitleBar minimal />, { beta: true, offlineLocked: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  await screen.findByRole('button', { name: 'Close' });
  expect(screen.queryByText('Update ready · installs when you close')).not.toBeInTheDocument();
});
