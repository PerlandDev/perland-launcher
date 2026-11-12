import { screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { Lobby } from './Lobby';

it('greets the player and shows every place the launcher has', async () => {
  renderApp(<Lobby />);
  expect(await screen.findByRole('heading', { name: 'Hi, Steve', level: 1 })).toBeInTheDocument();
  for (const name of ['Skins', 'Servers', 'Mods & more', 'Achievements']) {
    expect(screen.getByRole('heading', { name, level: 2 })).toBeInTheDocument();
  }
});

it('plays the last instance from its one lit key, and shows it starting on its own Home', async () => {
  const { user, calls } = renderApp(<Lobby />);
  const play = await screen.findByRole('button', { name: 'Play Asalith Fields' });
  expect(play).toHaveClass('btn-primary');
  await user.click(play);
  expect(routeSnapshot()).toEqual({ name: 'home', id: 'asalith-fields' });
  expect(sent(calls, 'play')).toEqual([expect.objectContaining({ id: 'asalith-fields' })]);
});

it('only opens the last instance while there is no account to play with', async () => {
  const { backend, user, calls } = renderApp(<Lobby />);
  backend.state.accounts.active = null;
  expect(await screen.findByRole('heading', { name: 'Welcome', level: 1 })).toBeInTheDocument();
  const open = await screen.findByRole('button', { name: 'Open Asalith Fields' });
  expect(open).not.toHaveClass('btn-primary');
  expect(screen.queryByRole('button', { name: 'Play Asalith Fields' })).toBeNull();
  await user.click(open);
  expect(routeSnapshot()).toEqual({ name: 'home', id: 'asalith-fields' });
  expect(sent(calls, 'play')).toEqual([]);
});

it('says how full the wardrobe and the catalog are, and opens Skins', async () => {
  const { user } = renderApp(<Lobby />);
  const skins = (await screen.findByRole('heading', { name: 'Skins' })).closest('article')!;
  expect(await within(skins).findByText(/2 in your wardrobe/)).toBeInTheDocument();
  await user.click(within(skins).getByRole('button', { name: 'Open wardrobe' }));
  expect(routeSnapshot()).toEqual({ name: 'skins' });
});

it('lists the three busiest servers and opens the whole list', async () => {
  const { user } = renderApp(<Lobby />);
  const servers = (await screen.findByRole('heading', { name: 'Servers' })).closest('article')!;
  const rows = await within(servers).findAllByRole('listitem');
  expect(rows.map((row) => within(row).getByRole('button').getAttribute('aria-label'))).toEqual([
    'Creative Plots, 20 / 20 playing',
    'Pure Vanilla, 12 / 50 playing',
    'Survival, 7 / 20 playing',
  ]);
  await user.click(within(servers).getByRole('button', { name: 'All servers' }));
  expect(routeSnapshot()).toEqual({ name: 'servers' });
});

it('stays calm while the server list is closed', async () => {
  renderApp(<Lobby />, { server: 'closed' });
  const servers = (await screen.findByRole('heading', { name: 'Servers' })).closest('article')!;
  expect(await within(servers).findByText("The server list isn't open yet.")).toBeInTheDocument();
  expect(within(servers).queryByRole('listitem')).toBeNull();
  expect(screen.queryByRole('alert')).toBeNull();
});

it('opens Mods & more on the kind picked', async () => {
  const { user } = renderApp(<Lobby />);
  const browse = (await screen.findByRole('heading', { name: 'Mods & more' })).closest('article')!;
  await user.click(within(browse).getByRole('button', { name: 'Shaders' }));
  expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'shader' });
});

it('shows the medal case with its count and opens Achievements', async () => {
  const { user } = renderApp(<Lobby />, { perland: 'linked' });
  const medals = (await screen.findByRole('heading', { name: 'Achievements' })).closest('article')!;
  await vi.waitFor(() => expect(within(medals).getByText('8 of 11 earned')).toBeInTheDocument());
  await user.click(within(medals).getByRole('button', { name: 'See all medals' }));
  expect(routeSnapshot()).toEqual({ name: 'achievements' });
});
