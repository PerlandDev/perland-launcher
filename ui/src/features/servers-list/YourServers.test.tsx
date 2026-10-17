import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { inviteOf } from '../../ipc/mockServers';
import { routeSnapshot } from '../../state/route';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { createMockBackend } from '../../ipc/mock';
import { ServersScreen } from './ServersScreen';

const NUDGE = 'Sign in with Perland to keep your servers here.';

/** The Your servers section, once it shows. */
const yours = () => screen.findByRole('region', { name: 'Your servers' });
const publicList = () => screen.findByRole('region', { name: 'Public servers' });
/** The card named `name` inside `section`. */
const cardIn = async (section: HTMLElement, name: string) => (await within(section).findByRole('heading', { name })).closest('article')!;

describe('YourServers', () => {
  it('nothing_when_accounts_off', async () => {
    const { calls } = renderApp(<ServersScreen />, { perland: 'off' });
    await cardIn(await publicList(), 'Survival');
    await vi.waitFor(() => expect(sent(calls, 'perland_status')).toHaveLength(1));
    expect(screen.queryByRole('region', { name: 'Your servers' })).toBeNull();
    expect(screen.queryByText(NUDGE)).toBeNull();
    expect(sent(calls, 'your_servers')).toEqual([]);
  });

  it('nudge_when_signed_out', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'signedOut' });
    expect(await screen.findByText(NUDGE)).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Your servers' })).toBeNull();
    // Signed out, the list is never asked for.
    expect(sent(calls, 'your_servers')).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Sign in' }));
    expect(routeSnapshot()).toEqual({ name: 'accounts' });
  });

  it('nothing_when_signed_in_and_empty', async () => {
    // Free servers off: with them on, the section stays for the free-server entry.
    const { calls } = renderApp(<ServersScreen />, { perland: 'empty', free: 'off' });
    await cardIn(await publicList(), 'Survival');
    await vi.waitFor(() => expect(sent(calls, 'your_servers')).toHaveLength(1));
    expect(screen.queryByRole('region', { name: 'Your servers' })).toBeNull();
    expect(screen.queryByText(NUDGE)).toBeNull();
  });

  it('shows_tags_owner_and_invited', async () => {
    renderApp(<ServersScreen />, { perland: 'signedIn' });
    const section = await yours();
    expect(within(section).getByRole('heading', { name: 'Your servers' })).toBeInTheDocument();
    expect(within(await cardIn(section, 'My world')).getByText('Owner')).toBeInTheDocument();
    expect(within(await cardIn(section, 'Friends')).getByText('Invited')).toBeInTheDocument();
    const followed = await cardIn(section, 'Quiet Cove');
    expect(within(followed).queryByText('Owner')).toBeNull();
    expect(within(followed).queryByText('Invited')).toBeNull();
    expect(screen.queryByText(NUDGE)).toBeNull();
  });

  it('shows the stats line only on a card with stats', async () => {
    renderApp(<ServersScreen />, { perland: 'linked' });
    const section = await yours();
    expect(await within(await cardIn(section, 'My world')).findByText(/^12 achievements · .+ played$/)).toBeInTheDocument();
    expect(within(await cardIn(section, 'Friends')).queryByText(/achievement/)).toBeNull();
  });

  it('menu_only_on_followed', async () => {
    // Without a panel, owned servers have nothing for a menu.
    renderApp(<ServersScreen />, { perland: 'signedIn', panel: false });
    const section = await yours();
    expect(within(await cardIn(section, 'My world')).queryByRole('button', { name: /^More for/ })).toBeNull();
    expect(within(await cardIn(section, 'Friends')).queryByRole('button', { name: /^More for/ })).toBeNull();
    expect(within(await cardIn(section, 'Quiet Cove')).getByRole('button', { name: 'More for Quiet Cove' })).toBeInTheDocument();
    expect(within(await cardIn(section, 'Pure Vanilla')).getByRole('button', { name: 'More for Pure Vanilla' })).toBeInTheDocument();
  });

  it('owned servers offer Manage in Perland', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'signedIn', free: 'has' });
    const section = await yours();
    await user.click(within(await cardIn(section, 'My world')).getByRole('button', { name: 'More for My world' }));
    await user.click(screen.getByRole('menuitem', { name: 'Manage in Perland' }));
    expect(sent(calls, 'perland_manage')).toEqual([{ stack: 'My world' }]);
    // The free server is the player's too.
    await user.click(within(await cardIn(section, 'Weekend SMP')).getByRole('button', { name: 'More for Weekend SMP' }));
    await user.click(screen.getByRole('menuitem', { name: 'Manage in Perland' }));
    expect(sent(calls, 'perland_manage')).toEqual([{ stack: 'My world' }, { stack: 'Weekend SMP' }]);
    // Invited and followed servers are someone else's.
    expect(within(await cardIn(section, 'Friends')).queryByRole('button', { name: /^More for/ })).toBeNull();
    await user.click(within(await cardIn(section, 'Quiet Cove')).getByRole('button', { name: 'More for Quiet Cove' }));
    expect(screen.queryByRole('menuitem', { name: 'Manage in Perland' })).toBeNull();
  });

  it('no Manage in Perland without a panel', async () => {
    const { user } = renderApp(<ServersScreen />, { perland: 'signedIn', free: 'has', panel: false });
    const section = await yours();
    expect(within(await cardIn(section, 'My world')).queryByRole('button', { name: /^More for/ })).toBeNull();
    await user.click(within(await cardIn(section, 'Weekend SMP')).getByRole('button', { name: 'More for Weekend SMP' }));
    expect(screen.getByRole('menuitem', { name: 'Delete server' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: 'Manage in Perland' })).toBeNull();
  });

  it('a refused Manage in Perland is a toast', async () => {
    const backend = createMockBackend({ perland: 'signedIn' });
    const { user } = renderWithProviders(<ServersScreen />, {
      ipc: (cmd, args) =>
        cmd === 'perland_manage'
          ? Promise.reject({ category: 'pack', code: 'link_refused', message: 'x', fix: null, detail: null, params: {} })
          : backend.handle(cmd, args),
    });
    await user.click(within(await cardIn(await yours(), 'My world')).getByRole('button', { name: 'More for My world' }));
    await user.click(screen.getByRole('menuitem', { name: 'Manage in Perland' }));
    await vi.waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
  });

  it('unfollow_removes_the_card', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'signedIn' });
    const section = await yours();
    await user.click(within(await cardIn(section, 'Quiet Cove')).getByRole('button', { name: 'More for Quiet Cove' }));
    await user.click(screen.getByRole('menuitem', { name: 'Unfollow' }));
    expect(sent(calls, 'your_servers_unfollow')).toEqual([{ invite: inviteOf('K') }]);
    await vi.waitFor(() => expect(within(section).queryByRole('heading', { name: 'Quiet Cove' })).toBeNull());
    expect(within(section).getByRole('heading', { name: 'My world' })).toBeInTheDocument();
  });

  it('unfollow keeps focus on Your servers', async () => {
    const { user } = renderApp(<ServersScreen />, { perland: 'signedIn' });
    const section = await yours();
    await user.click(within(await cardIn(section, 'Quiet Cove')).getByRole('button', { name: 'More for Quiet Cove' }));
    await user.click(screen.getByRole('menuitem', { name: 'Unfollow' }));
    await vi.waitFor(() => expect(within(section).queryByRole('heading', { name: 'Quiet Cove' })).toBeNull());
    await vi.waitFor(() => expect(document.activeElement).toBe(within(section).getByRole('heading', { name: 'Your servers' })));
  });

  it('unfollowing the last card moves focus to Public servers', async () => {
    const { user, backend } = renderApp(<ServersScreen />, { perland: 'signedIn', free: 'off' });
    backend.state.perland.servers = backend.state.perland.servers.filter((s) => s.name === 'Quiet Cove');
    const section = await yours();
    await user.click(within(await cardIn(section, 'Quiet Cove')).getByRole('button', { name: 'More for Quiet Cove' }));
    await user.click(screen.getByRole('menuitem', { name: 'Unfollow' }));
    await vi.waitFor(() => expect(screen.queryByRole('region', { name: 'Your servers' })).toBeNull());
    const list = await publicList();
    await vi.waitFor(() => expect(document.activeElement).toBe(within(list).getByRole('heading', { name: 'Public servers' })));
  });

  it('unfollow on a public card moves focus to its Play', async () => {
    const { user } = renderApp(<ServersScreen />, { perland: 'signedIn' });
    const pure = await cardIn(await publicList(), 'Pure Vanilla');
    await user.click(await within(pure).findByRole('button', { name: 'More for Pure Vanilla' }));
    await user.click(screen.getByRole('menuitem', { name: 'Unfollow' }));
    await vi.waitFor(() => expect(within(pure).queryByRole('button', { name: 'More for Pure Vanilla' })).toBeNull());
    await vi.waitFor(() => expect(document.activeElement).toBe(within(pure).getByRole('button', { name: 'Play' })));
  });

  it('a failed unfollow is a toast', async () => {
    const backend = createMockBackend({ perland: 'signedIn' });
    const { user } = renderWithProviders(<ServersScreen />, {
      ipc: (cmd, args) =>
        cmd === 'your_servers_unfollow'
          ? Promise.reject({ category: 'network', code: 'host_unreachable', message: 'x', fix: null, detail: null, params: { host: 'play.example.com' } })
          : backend.handle(cmd, args),
    });
    const section = await yours();
    await user.click(within(await cardIn(section, 'Quiet Cove')).getByRole('button', { name: 'More for Quiet Cove' }));
    await user.click(screen.getByRole('menuitem', { name: 'Unfollow' }));
    await vi.waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
    expect(within(section).getByRole('heading', { name: 'Quiet Cove' })).toBeInTheDocument();
  });

  it('public_play_follows', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'signedIn' });
    await user.click(within(await cardIn(await publicList(), 'Creative Plots')).getByRole('button', { name: 'Play' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'creative-plots' }));
    expect(sent(calls, 'your_servers_follow')).toEqual([{ invite: inviteOf('B') }]);
  });

  it('followed_play_follows', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'signedIn' });
    await user.click(within(await cardIn(await yours(), 'Quiet Cove')).getByRole('button', { name: 'Play' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'quiet-cove' }));
    expect(sent(calls, 'your_servers_follow')).toEqual([{ invite: inviteOf('K') }]);
  });

  it('owner_play_does_not_follow', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'signedIn' });
    const section = await yours();
    await user.click(within(await cardIn(section, 'My world')).getByRole('button', { name: 'Play' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'my-world' }));
    await user.click(within(await cardIn(section, 'Friends')).getByRole('button', { name: 'Play' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'friends' }));
    expect(sent(calls, 'join_add')).toEqual([{ link: inviteOf('O') }, { link: inviteOf('I') }]);
    expect(sent(calls, 'your_servers_follow')).toEqual([]);
  });

  it('a failed follow is never shown and Play still opens Home', async () => {
    const backend = createMockBackend({ perland: 'signedIn' });
    const { user } = renderWithProviders(<ServersScreen />, {
      ipc: (cmd, args) =>
        cmd === 'your_servers_follow'
          ? Promise.reject({ category: 'network', code: 'host_unreachable', message: 'x', fix: null, detail: null, params: { host: 'play.example.com' } })
          : backend.handle(cmd, args),
    });
    await user.click(within(await cardIn(await publicList(), 'Creative Plots')).getByRole('button', { name: 'Play' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'creative-plots' }));
    expect(toastsSnapshot()).toEqual([]);
  });

  it('public_card_has_menu_when_followed', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'signedIn' });
    const list = await publicList();
    const pure = await cardIn(list, 'Pure Vanilla');
    expect(await within(pure).findByRole('button', { name: 'More for Pure Vanilla' })).toBeInTheDocument();
    expect(within(await cardIn(list, 'Survival')).queryByRole('button', { name: /^More for/ })).toBeNull();
    await user.click(within(pure).getByRole('button', { name: 'More for Pure Vanilla' }));
    await user.click(screen.getByRole('menuitem', { name: 'Unfollow' }));
    expect(sent(calls, 'your_servers_unfollow')).toEqual([{ invite: inviteOf('F') }]);
    // Both lists read again: the Your servers card leaves and the public card loses its menu.
    await vi.waitFor(() => expect(within(pure).queryByRole('button', { name: 'More for Pure Vanilla' })).toBeNull());
    expect(within(await yours()).queryByRole('heading', { name: 'Pure Vanilla' })).toBeNull();
  });

  it('offline_card_reads_offline', async () => {
    renderApp(<ServersScreen />, { perland: 'signedIn' });
    const mine = await cardIn(await yours(), 'My world');
    expect(within(mine).getByText('Offline')).toBeInTheDocument();
    expect(within(mine).getByRole('button', { name: 'Play' })).toBeEnabled();
  });

  it('a session the Engine ended reads as signed out', async () => {
    const backend = createMockBackend({ perland: 'signedIn' });
    const statuses: unknown[] = [];
    renderWithProviders(<ServersScreen />, {
      ipc: async (cmd, args) => {
        if (cmd === 'your_servers') {
          // A 401 on /servers: the backend forgets the session and answers null.
          await backend.handle('perland_sign_out', {});
          return null;
        }
        if (cmd === 'perland_status') statuses.push(cmd);
        return backend.handle(cmd, args);
      },
    });
    expect(await screen.findByText(NUDGE)).toBeInTheDocument();
    expect(statuses).toHaveLength(2);
    expect(screen.queryByRole('region', { name: 'Your servers' })).toBeNull();
  });

  it('shows Your servers in Spanish', async () => {
    const { default: i18n } = await import('../../i18n');
    await i18n.changeLanguage('es');
    const { user } = renderApp(<ServersScreen />, { perland: 'signedIn' });
    const section = await screen.findByRole('region', { name: 'Tus servidores' });
    expect(within(await cardIn(section, 'My world')).getByText('Dueño')).toBeInTheDocument();
    expect(within(await cardIn(section, 'Friends')).getByText('Invitado')).toBeInTheDocument();
    expect(within(await cardIn(section, 'Quiet Cove')).getByRole('button', { name: 'Más opciones para Quiet Cove' })).toBeInTheDocument();
    await user.click(within(await cardIn(section, 'My world')).getByRole('button', { name: 'Más opciones para My world' }));
    expect(screen.getByRole('menuitem', { name: 'Administrar en Perland' })).toBeInTheDocument();
  });

  it('nudges in Spanish', async () => {
    const { default: i18n } = await import('../../i18n');
    await i18n.changeLanguage('es');
    renderApp(<ServersScreen />, { perland: 'signedOut' });
    expect(await screen.findByText('Inicia sesión con Perland para guardar tus servidores aquí.')).toBeInTheDocument();
  });
});
