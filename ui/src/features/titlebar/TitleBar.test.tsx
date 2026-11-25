import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { markPersonalizeSeen, resetPersonalizeSeen } from '../../state/personalizeSeen';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { TitleBar } from './TitleBar';

afterEach(() => {
  vi.restoreAllMocks();
  resetPersonalizeSeen();
});

const brush = () => screen.findByRole('button', { name: 'Personalize' });
/** Long enough for the settings to arrive from the mock, so "no mark" is not just "not loaded yet". */
const settled = () => new Promise((resolve) => setTimeout(resolve, 50));

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
  expect(places.map((p) => p.textContent)).toEqual(['Skins', 'Servers', 'Mods & more', 'Medals and achievements']);
  for (const place of places) expect(place.compareDocumentPosition(account) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  // The account is followed by Personalize and then Settings: Skins is a tab now, not an icon beside it.
  const buttons = screen.getAllByRole('button');
  expect(buttons.indexOf(screen.getByRole('button', { name: 'Personalize' }))).toBe(buttons.indexOf(account) + 1);
  expect(buttons.indexOf(screen.getByRole('button', { name: 'Settings' }))).toBe(buttons.indexOf(account) + 2);
});

it('opens each place and marks the one that is open', async () => {
  const { user } = renderApp(<TitleBar />);
  await screen.findByRole('button', { name: /Steve/ });
  // On an instance's Home (where the launcher starts) no place is open: the rail marks the instance.
  for (const name of ['Skins', 'Servers', 'Mods & more', 'Medals and achievements']) expect(screen.getByRole('button', { name })).not.toHaveAttribute('aria-current');
  await user.click(screen.getByRole('button', { name: 'Servers' }));
  expect(routeSnapshot()).toEqual({ name: 'servers' });
  expect(screen.getByRole('button', { name: 'Servers' })).toHaveAttribute('aria-current', 'page');
  await user.click(screen.getByRole('button', { name: 'Mods & more' }));
  expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'modpack' });
  await user.click(screen.getByRole('button', { name: 'Skins' }));
  expect(routeSnapshot()).toEqual({ name: 'skins' });
  expect(screen.getByRole('button', { name: 'Skins' })).toHaveAttribute('aria-current', 'page');
  expect(screen.getByRole('button', { name: 'Servers' })).not.toHaveAttribute('aria-current');
  await user.click(screen.getByRole('button', { name: 'Medals and achievements' }));
  expect(routeSnapshot()).toEqual({ name: 'achievements' });
  expect(screen.getByRole('button', { name: 'Medals and achievements' })).toHaveAttribute('aria-current', 'page');
  expect(screen.getByRole('button', { name: 'Skins' })).not.toHaveAttribute('aria-current');
});

it('walks the places with the arrow keys from one tab stop', async () => {
  const { user } = renderApp(<TitleBar />);
  await screen.findByRole('button', { name: /Steve/ });
  await user.tab();
  expect(screen.getByRole('button', { name: 'Skins' })).toHaveFocus();
  await user.keyboard('{ArrowRight}');
  expect(screen.getByRole('button', { name: 'Servers' })).toHaveFocus();
  await user.keyboard('{End}');
  expect(screen.getByRole('button', { name: 'Medals and achievements' })).toHaveFocus();
  await user.keyboard('{ArrowLeft}');
  expect(screen.getByRole('button', { name: 'Mods & more' })).toHaveFocus();
  await user.keyboard('{End}');
  // One stop for the places: the next Tab is the search key (Go to), then the account.
  await user.tab();
  expect(screen.getByRole('button', { name: 'Search' })).toHaveFocus();
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

it('keeps the title bar clear when an update is ready: it is told in the Activity bar', async () => {
  renderApp(<TitleBar />, { beta: true, offlineLocked: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  await screen.findByRole('button', { name: 'Close' });
  expect(screen.queryByRole('button', { name: 'Update ready' })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Restart now' })).toBeNull();
});

it('marks the brush as new until Personalize has been opened, and remembers that', async () => {
  renderApp(<TitleBar />);
  const key = await brush();
  // The name stays "Personalize"; the mark is described, so a screen reader hears it after the name.
  await waitFor(() => expect(key).toHaveAccessibleDescription('New'));
  expect(key.querySelector('.tb-new')).not.toBeNull();
  act(() => markPersonalizeSeen());
  expect(key).not.toHaveAccessibleDescription();
  expect(key.querySelector('.tb-new')).toBeNull();
  // Next launch: still gone.
  cleanup();
  resetPersonalizeSeen({ keepStored: true });
  renderApp(<TitleBar />);
  await settled();
  expect(await brush()).not.toHaveAccessibleDescription();
});

it('does not mark the brush for a player who has already changed the theme', async () => {
  renderApp(<TitleBar />, { theme: 'night' });
  await screen.findByRole('button', { name: /Steve/ });
  await settled();
  expect(await brush()).not.toHaveAccessibleDescription();
  expect((await brush()).querySelector('.tb-new')).toBeNull();
});

it('does not mark the brush before first run is done', async () => {
  renderApp(<TitleBar />, { firstRun: true });
  await screen.findByRole('button', { name: 'Add account' });
  await settled();
  expect(await brush()).not.toHaveAccessibleDescription();
});

it('leaves the brush plain when the launcher cannot remember', async () => {
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
    throw new Error('blocked');
  });
  renderApp(<TitleBar />);
  await screen.findByRole('button', { name: /Steve/ });
  await settled();
  expect(await brush()).not.toHaveAccessibleDescription();
});
