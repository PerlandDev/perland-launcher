import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import { inviteOf } from '../../ipc/mockServers';
import type { PublicServerCard as Card } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { PublicServerCard } from '../servers-list/PublicServerCard';
import { ServersScreen } from '../servers-list/ServersScreen';

const ENTRY = 'Create a server for friends';
const yours = () => screen.findByRole('region', { name: 'Your servers' });
const cardIn = async (section: HTMLElement, name: string) => (await within(section).findByRole('heading', { name })).closest('article')!;

describe('FreeServerCard', () => {
  it('free_card_first_with_owner_tag', async () => {
    renderApp(<ServersScreen />, { perland: 'signedIn', free: 'has' });
    const section = await yours();
    const card = await cardIn(section, 'Weekend SMP');
    expect(within(card).getByText('Owner')).toBeInTheDocument();
    expect(within(card).getByText('Sleeping, Play wakes it')).toBeInTheDocument();
    const items = within(section).getAllByRole('listitem');
    expect(items[0]).toContainElement(card);
    // Drawn once.
    expect(within(section).getAllByRole('heading', { name: 'Weekend SMP' })).toHaveLength(1);
    expect(within(card).getByRole('button', { name: 'Play' })).toBeEnabled();
  });

  it('warned_note', async () => {
    const { backend } = renderApp(<ServersScreen />, { perland: 'signedIn', free: 'warned' });
    const deletesAt = backend.state.free.server!.deletion!.deletesAt;
    const date = new Intl.DateTimeFormat('en', { day: 'numeric', month: 'short' }).format(new Date(deletesAt));
    const card = await cardIn(await yours(), 'Weekend SMP');
    expect(within(card).getByText(`Deleted on ${date} unless someone plays.`)).toBeInTheDocument();
  });

  it('no notes on a server that is fine', async () => {
    renderApp(<ServersScreen />, { perland: 'signedIn', free: 'has' });
    const card = await cardIn(await yours(), 'Weekend SMP');
    expect(within(card).queryByText(/unless someone plays/)).toBeNull();
    expect(within(card).queryByText("It's out of space and stays off.")).toBeNull();
  });

  it('over_disk_note_and_play_disabled', async () => {
    renderApp(<ServersScreen />, { perland: 'signedIn', free: 'overDisk' });
    const card = await cardIn(await yours(), 'Weekend SMP');
    expect(within(card).getByText("It's out of space and stays off.")).toBeInTheDocument();
    expect(within(card).getByText('Offline')).toBeInTheDocument();
    expect(within(card).getByRole('button', { name: 'Play' })).toBeDisabled();
  });

  it('creating_status_word_play_disabled', async () => {
    renderApp(<ServersScreen />, { perland: 'signedIn', free: 'creating' });
    const card = await cardIn(await yours(), 'Weekend SMP');
    expect(within(card).getByText('Being created…')).toBeInTheDocument();
    expect(within(card).getByText('Owner')).toBeInTheDocument();
    expect(within(card).getByRole('button', { name: 'Play' })).toBeDisabled();
  });

  it('deleting reads Being deleted… with Play disabled', async () => {
    const { backend } = renderApp(<ServersScreen />, { perland: 'signedIn', free: 'has' });
    backend.state.free.server = { ...backend.state.free.server!, freeStatus: 'deleting', status: 'sleeping', joinable: false };
    const card = await cardIn(await yours(), 'Weekend SMP');
    expect(within(card).getByText('Being deleted…')).toBeInTheDocument();
    expect(within(card).getByRole('button', { name: 'Play' })).toBeDisabled();
  });

  it('play_uses_free_server_play', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'signedIn', free: 'has' });
    const card = await cardIn(await yours(), 'Weekend SMP');
    await user.click(within(card).getByRole('button', { name: 'Play' }));
    await vi.waitFor(() => expect(routeSnapshot().name).toBe('home'));
    expect(sent(calls, 'free_server_play')).toHaveLength(1);
    const route = routeSnapshot() as { name: 'home'; id: string };
    expect(sent(calls, 'play')).toEqual([expect.objectContaining({ id: route.id })]);
    // Never the public join path for the free server, and never a follow.
    expect(sent(calls, 'join_add')).toEqual([]);
    expect(sent(calls, 'your_servers_follow')).toEqual([]);
  });

  it('an over-disk answer to Play is a toast', async () => {
    const backend = createMockBackend({ perland: 'signedIn', free: 'has' });
    const { user } = renderWithProviders(<ServersScreen />, {
      ipc: (cmd, args) =>
        cmd === 'free_server_play'
          ? Promise.reject({
              category: 'network',
              code: 'free_server_over_disk',
              message: 'x',
              fix: null,
              detail: null,
              params: { usedBytes: '1', limitBytes: '1' },
            })
          : backend.handle(cmd, args),
    });
    const card = await cardIn(await yours(), 'Weekend SMP');
    await user.click(within(card).getByRole('button', { name: 'Play' }));
    await vi.waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
    expect(toastsSnapshot()[0]?.error).toMatchObject({ code: 'free_server_over_disk' });
  });

  it('entry_shows_when_no_server', async () => {
    renderApp(<ServersScreen />, { perland: 'signedIn', free: 'none' });
    const section = await yours();
    expect(await within(section).findByRole('button', { name: ENTRY })).toBeInTheDocument();
    expect(within(section).queryByRole('heading', { name: 'Weekend SMP' })).toBeNull();
  });

  it('the free card alone still shows in a list', async () => {
    renderApp(<ServersScreen />, { perland: 'empty', free: 'has' });
    const section = await yours();
    const card = await cardIn(section, 'Weekend SMP');
    expect(within(section).getAllByRole('listitem')).toHaveLength(1);
    expect(within(section).getAllByRole('listitem')[0]).toContainElement(card);
  });

  it('nothing_when_free_servers_off', async () => {
    const { calls } = renderApp(<ServersScreen />, { perland: 'signedIn', free: 'off' });
    const section = await yours();
    await vi.waitFor(() => expect(sent(calls, 'free_server').length).toBeGreaterThan(0));
    expect(within(section).queryByRole('heading', { name: 'Weekend SMP' })).toBeNull();
    expect(within(section).queryByRole('button', { name: ENTRY })).toBeNull();
    expect(screen.queryByRole('menuitem', { name: 'Delete server' })).toBeNull();
  });

  it('shows the card in Spanish', async () => {
    const { default: i18n } = await import('../../i18n');
    await i18n.changeLanguage('es');
    renderApp(<ServersScreen />, { perland: 'signedIn', free: 'overDisk' });
    const section = await screen.findByRole('region', { name: 'Tus servidores' });
    const card = await cardIn(section, 'Weekend SMP');
    expect(within(card).getByText('Se quedó sin espacio y seguirá apagado.')).toBeInTheDocument();
    expect(within(card).getByText('Dueño')).toBeInTheDocument();
  });
});

describe('PublicServerCard', () => {
  const card: Card = {
    token: 'Q'.repeat(43),
    invite: inviteOf('Q'),
    joinable: true,
    address: 'play.example.com:25590',
    name: 'Plain',
    iconVersion: null,
    game: { version: '1.21.1', loader: null, loaderVersion: null },
    modded: false,
    status: 'online',
    players: { online: 2, max: 20 },
    whitelisted: false,
    pack: null,
    categories: [],
    language: '',
    region: null,
    instance: null,
  };

  it('public_card_unchanged_without_new_props', async () => {
    const backend = createMockBackend({});
    renderWithProviders(<PublicServerCard card={card} />, { ipc: (cmd, args) => backend.handle(cmd, args) });
    const article = (await screen.findByRole('heading', { name: 'Plain' })).closest('article')!;
    expect(within(article).getByText('2 / 20 playing')).toBeInTheDocument();
    expect(within(article).getByText('Vanilla 1.21.1')).toBeInTheDocument();
    expect(within(article).queryByRole('button', { name: /^More for/ })).toBeNull();
    expect(within(article).queryByRole('note')).toBeNull();
    expect(article.querySelector('.server-tile-note')).toBeNull();
    expect(within(article).queryByText('Owner')).toBeNull();
    const play = within(article).getByRole('button', { name: 'Play' });
    expect(play).toBeEnabled();
    expect(within(article).getAllByRole('button')).toHaveLength(1);
  });
});
