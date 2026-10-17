import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import App from '../../App';
import { createMockBackend } from '../../ipc/mock';
import { MOCK_PUBLIC_SERVERS } from '../../ipc/mockServers';
import type { InstanceListing, ListQuery, PublicServerPage } from '../../ipc/schema';
import { navigate, routeSnapshot } from '../../state/route';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ServersScreen } from './ServersScreen';

const B_INVITE = `https://play.example.com/${'B'.repeat(43)}`;

/** The card (an article) named by its server's name. */
const card = async (name: string) => (await screen.findByRole('heading', { name })).closest('article')!;

/** Render with the mock backend, letting `servers_list` answers be changed or replaced. */
function renderWith(answer: (page: Promise<PublicServerPage>) => Promise<unknown>) {
  const backend = createMockBackend();
  const calls: [string, unknown][] = [];
  const result = renderWithProviders(<ServersScreen />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      const reply = backend.handle(cmd, args);
      return cmd === 'servers_list' ? answer(reply as Promise<PublicServerPage>) : reply;
    },
  });
  return { ...result, calls };
}

describe('ServersScreen', () => {
  it("the rail's Servers button opens the screen", async () => {
    const { user } = renderApp(<App />);
    await user.click(await screen.findByRole('button', { name: 'Servers' }));
    expect(routeSnapshot()).toEqual({ name: 'servers' });
    expect(await screen.findByRole('heading', { name: 'Servers', level: 1 })).toBeInTheDocument();
    expect(screen.getByText('Find a Perland server and press Play.')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Public servers' })).toBeInTheDocument();
  });

  it('cards show name, game line, players and Play', async () => {
    renderApp(<ServersScreen />);
    const survival = await card('Survival');
    expect(within(survival).getByText('Survival Pack 1.0.4')).toBeInTheDocument();
    expect(within(survival).getByText('7 / 20 playing')).toBeInTheDocument();
    expect(within(survival).getByRole('button', { name: 'Play' })).toBeEnabled();
    const creative = await card('Creative Plots');
    expect(within(creative).getByText('Vanilla 1.21.1')).toBeInTheDocument();
  });

  it('a sleeping server reads "Sleeping, Play wakes it"', async () => {
    renderApp(<ServersScreen />);
    const sleepy = await card('Sleepy Hollow');
    expect(within(sleepy).getByText('Sleeping, Play wakes it')).toBeInTheDocument();
    expect(within(sleepy).getByRole('button', { name: 'Play' })).toBeEnabled();
    expect(within(await card('Skyblock Dawn')).getByText('Starting')).toBeInTheDocument();
  });

  it('a whitelisted server shows Invite only', async () => {
    renderApp(<ServersScreen />);
    expect(await within(await card('The Whitelist Club')).findByText('Invite only: ask the owner to add you (Steve)')).toBeInTheDocument();
    expect(within(await card('Survival')).queryByText(/Invite only/)).toBeNull();
  });

  it('filters send their query', async () => {
    const { user, calls } = renderApp(<ServersScreen />);
    await card('Survival');
    await user.selectOptions(screen.getByLabelText('Mods'), 'Without mods');
    await vi.waitFor(() => expect((sent(calls, 'servers_list').at(-1) as { query: ListQuery }).query.modded).toBe(false));
    expect(await card('Creative Plots')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Survival' })).toBeNull();

    await user.click(screen.getByRole('button', { name: 'Has room' }));
    expect(screen.getByRole('button', { name: 'Has room' })).toHaveAttribute('aria-pressed', 'true');
    await user.selectOptions(screen.getByLabelText('Sort by'), 'Newest');
    await vi.waitFor(() =>
      expect((sent(calls, 'servers_list').at(-1) as { query: ListQuery }).query).toMatchObject({ sort: 'newest', hasRoom: true, modded: false, offset: 0 }),
    );
    // The mock lists three regions, so Region is offered.
    expect(screen.getByLabelText('Region')).toBeInTheDocument();
  });

  it('Region is hidden with fewer than two regions', async () => {
    renderWith(async (page) => {
      const p = await page;
      return { ...p, facets: { ...p.facets, regions: ['eu'] } };
    });
    await card('Survival');
    expect(screen.getByLabelText('Language')).toBeInTheDocument();
    expect(screen.queryByLabelText('Region')).toBeNull();
  });

  it('Play joins the server and opens its Home', async () => {
    const { user, calls } = renderApp(<ServersScreen />);
    await user.click(within(await card('Creative Plots')).getByRole('button', { name: 'Play' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'creative-plots' }));
    expect(sent(calls, 'join_add')).toEqual([{ link: B_INVITE }]);
    expect(sent(calls, 'play')).toEqual([{ id: 'creative-plots', join: true }]);
    const order = calls.map(([cmd]) => cmd).filter((cmd) => cmd === 'join_add' || cmd === 'play');
    expect(order).toEqual(['join_add', 'play']);
  });

  it('Play reuses the instance that already joins the server', async () => {
    // The seeded `survival` instance joins the Survival card's invite.
    const { user, calls, backend } = renderApp(<ServersScreen />, { server: 'online' });
    const ids = async () => ((await backend.handle('instances_list', {})) as InstanceListing).instances.map((i) => i.id);
    const before = await ids();
    await user.click(within(await card('Survival')).getByRole('button', { name: 'Play' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' }));
    // join_add is always asked (it answers the instance that already joins), so a stale id is never played.
    expect(sent(calls, 'join_add')).toEqual([{ link: MOCK_PUBLIC_SERVERS.find((c) => c.name === 'Survival')!.invite }]);
    expect(sent(calls, 'play')).toEqual([{ id: 'survival', join: true }]);
    expect(await ids()).toEqual(before);
  });

  it('a server already running opens its Home instead of a toast', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { server: 'online' });
    const play = within(await card('Survival')).getByRole('button', { name: 'Play' });
    await user.click(play);
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' }));
    navigate({ name: 'servers' });
    await vi.waitFor(() => expect(play).not.toHaveAttribute('aria-disabled'));
    await user.click(play);
    await vi.waitFor(() => expect(sent(calls, 'play')).toHaveLength(2));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' }));
    expect(toastsSnapshot()).toEqual([]);
  });

  it('a failed Play is a toast and the card stays', async () => {
    const backend = createMockBackend();
    const { user } = renderWithProviders(<ServersScreen />, {
      ipc: (cmd, args) =>
        cmd === 'join_add'
          ? Promise.reject({ category: 'network', code: 'host_unreachable', message: 'x', fix: null, detail: null, params: { host: 'play.example.com' } })
          : backend.handle(cmd, args),
    });
    const creative = await card('Creative Plots');
    await user.click(within(creative).getByRole('button', { name: 'Play' }));
    await vi.waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
    expect(routeSnapshot()).toEqual({ name: 'home', id: null });
    expect(within(creative).getByRole('button', { name: 'Play' })).not.toHaveAttribute('aria-disabled');
  });

  it("a card that can't be joined shows the address and a disabled Play", async () => {
    renderApp(<ServersScreen />);
    const forge = await card('Tech Forge');
    expect(within(forge).getByText('Forge 1.20.1')).toBeInTheDocument();
    expect(within(forge).getByText('Join from Minecraft: play.example.com:25572')).toBeInTheDocument();
    expect(within(forge).getByRole('button', { name: 'Copy server address' })).toBeInTheDocument();
    expect(within(forge).getByRole('button', { name: 'Play' })).toBeDisabled();
  });

  it('a closed list shows a calm message', async () => {
    renderApp(<ServersScreen />, { server: 'closed' });
    expect(await screen.findByText("The server list isn't open yet. Check back soon.")).toBeInTheDocument();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Try again' })).toBeNull();
    expect(toastsSnapshot()).toEqual([]);
  });

  it('no matches offers Clear filters', async () => {
    const { user } = renderApp(<ServersScreen />);
    await card('Survival');
    await user.selectOptions(screen.getByLabelText('Minecraft version'), '1.20.1');
    await user.selectOptions(screen.getByLabelText('Language'), 'Español');
    expect(await screen.findByText('No servers match. Try fewer filters.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Clear filters' }));
    expect(await card('Survival')).toBeInTheDocument();
    expect(screen.getByLabelText('Language')).toHaveValue('');
  });

  it('an empty list says no public servers yet', async () => {
    renderWith(async (page) => ({ ...(await page), servers: [], total: 0 }));
    expect(await screen.findByText('No public servers yet.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Clear filters' })).toBeNull();
  });

  it('an offline error shows Retry', async () => {
    let offline = true;
    const { user, calls } = renderWith((page) =>
      offline
        ? Promise.reject({ category: 'network', code: 'host_unreachable', message: "Couldn't reach play.example.com", fix: null, detail: null, params: { host: 'play.example.com' } })
        : page,
    );
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    offline = false;
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await card('Survival')).toBeInTheDocument();
    expect(sent(calls, 'servers_list').length).toBeGreaterThanOrEqual(2);
  });

  it('asks for an icon only when the card has one', async () => {
    const { calls } = renderApp(<ServersScreen />);
    await card('Survival');
    await vi.waitFor(() => expect(sent(calls, 'servers_icon')).toContainEqual({ token: 'A'.repeat(43), iconVersion: '1a2b3c4d5e6f7a8b' }));
    expect(sent(calls, 'servers_icon').some((args) => (args as { token: string }).token === 'C'.repeat(43))).toBe(false);
  });

  it('shows the screen in Spanish', async () => {
    const { default: i18n } = await import('../../i18n');
    await i18n.changeLanguage('es');
    renderApp(<ServersScreen />);
    expect(await screen.findByRole('heading', { name: 'Servidores', level: 1 })).toBeInTheDocument();
    expect(within(await card('Sleepy Hollow')).getByText('Dormido, Jugar lo despierta')).toBeInTheDocument();
    expect(within(await card('Survival')).getByText('7 / 20 jugando')).toBeInTheDocument();
  });
});
