import { act, fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import { MOCK_INVITE } from '../../ipc/mockServers';
import type { Instance } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { JoinServer } from './JoinServer';

const OTHER = `https://play.example.com/${'B'.repeat(43)}`;

describe('JoinServer', () => {
  it('previews a server and adds it', async () => {
    const { user, calls } = renderApp(<JoinServer invite={OTHER} />, { server: 'online' });
    expect(await screen.findByRole('heading', { name: 'survival' })).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Online · 3/20');
    expect(screen.getByText('From play.example.com')).toBeInTheDocument();
    expect(screen.getByText('Survival Pack 1.0.4 · 142 mods')).toBeInTheDocument();
    expect(screen.getByText('Minecraft 1.20.1')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Add server' }));
    expect(sent(calls, 'join_add')).toEqual([{ link: OTHER }]);
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival-2' }));
  });

  it('asks the launcher about the canonical link, so spellings of one link share a cache entry', async () => {
    const { calls } = renderApp(<JoinServer invite={`https://PLAY.example.com/${'B'.repeat(43)}/`} />, { server: 'online' });
    expect(await screen.findByRole('heading', { name: 'survival' })).toBeInTheDocument();
    expect(sent(calls, 'join_preview')).toEqual([{ link: OTHER }]);
  });

  it("falls back to the generated icon when the server's icon fails to decode", async () => {
    renderApp(<JoinServer invite={OTHER} />, { server: 'online' });
    const card = (await screen.findByRole('heading', { name: 'survival' })).closest('article')!;
    const icon = card.querySelector('img')!;
    // 64 px at 2 device px per icon px (jsdom is 1 dppx).
    expect(icon.style.width).toBe('128px');
    fireEvent.error(icon);
    expect(card.querySelector('img')).toBeNull();
    expect(card.querySelector('.server-icon svg')).not.toBeNull();
  });

  it('offers Open for a server that is already added', async () => {
    const { user, calls } = renderApp(<JoinServer invite={MOCK_INVITE} />, { server: 'online' });
    await user.click(await screen.findByRole('button', { name: 'Open' }));
    expect(sent(calls, 'join_add')).toEqual([]);
    expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' });
  });

  it('says when the server is invite only, naming the active account', async () => {
    renderApp(<JoinServer invite={OTHER} />, { server: 'whitelisted' });
    expect(await screen.findByText('Invite only: ask the owner to add Steve')).toBeInTheDocument();
  });

  it('shows a vanilla server without a pack line', async () => {
    renderApp(<JoinServer invite={OTHER} />, { server: 'vanilla' });
    expect(await screen.findByText('Vanilla Minecraft 1.21.1')).toBeInTheDocument();
    expect(screen.queryByText(/mods$/)).not.toBeInTheDocument();
  });

  it('refuses servers it cannot join but gives their address to copy', async () => {
    const { user } = renderApp(<JoinServer invite={OTHER} />, { server: 'network' });
    expect(
      await screen.findByText("This server can't be joined from the launcher yet. Join with the address play.example.com:25577."),
    ).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Copy address' }));
    expect(await navigator.clipboard.readText()).toBe('play.example.com:25577');
  });

  it.each([
    ['network', "This server can't be joined from the launcher yet. Join with the address play.example.com:25577."],
    ['gone', "This invite link doesn't work anymore. Ask the owner for a new one."],
  ] as const)('offers a way out when the link fails (%s)', async (server, message) => {
    const { user } = renderApp(<JoinServer invite={OTHER} />, { server });
    expect(await screen.findByText(message)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(routeSnapshot()).toEqual({ name: 'home', id: null });
  });

  it('offers a way out of relinking before a link is checked', async () => {
    const { user } = renderApp(<JoinServer invite="" relinkFor="survival" />, { server: 'online' });
    await user.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' });
  });

  it('says when the link is dead', async () => {
    renderApp(<JoinServer invite={OTHER} />, { server: 'gone' });
    expect(await screen.findByText("This invite link doesn't work anymore. Ask the owner for a new one.")).toBeInTheDocument();
  });

  it('checks a pasted link before asking the launcher', async () => {
    const { user, calls } = renderApp(<JoinServer invite="" />, { server: 'online' });
    // The WebView would otherwise block submit on a non-URL with its own bubble (jsdom doesn't), hiding our error.
    expect(screen.getByRole('textbox', { name: 'Invite link' }).closest('form')?.noValidate).toBe(true);
    await user.type(screen.getByRole('textbox', { name: 'Invite link' }), 'abc');
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(screen.getByText("That isn't a Perland invite link.")).toBeInTheDocument();
    await user.clear(screen.getByRole('textbox', { name: 'Invite link' }));
    await user.type(screen.getByRole('textbox', { name: 'Invite link' }), 'https://play.example.com/short');
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(screen.getByText("That isn't a Perland invite link.")).toBeInTheDocument();
    expect(sent(calls, 'join_preview')).toEqual([]);
    await user.clear(screen.getByRole('textbox', { name: 'Invite link' }));
    await user.type(screen.getByRole('textbox', { name: 'Invite link' }), OTHER);
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(await screen.findByRole('heading', { name: 'survival' })).toBeInTheDocument();
  });

  it('relinks, warning when the link is for another server', async () => {
    const { user, calls } = renderApp(<JoinServer invite="" relinkFor="survival" />, { server: 'vanilla' });
    expect(await screen.findByRole('heading', { name: 'Use a new invite link' })).toBeInTheDocument();
    await user.type(screen.getByRole('textbox', { name: 'Invite link' }), OTHER);
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(await screen.findByText('This link is for creative, not survival.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Use this link' }));
    expect(sent(calls, 'server_relink')).toEqual([{ id: 'survival', link: OTHER }]);
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' }));
  });

  it('does not call a long name another server: the saved name is cut to 64 characters', async () => {
    // 66 characters, the first two outside the Basic Multilingual Plane (two UTF-16 units each).
    const long = `🌻🌻 ${'Survival '.repeat(7)}`.trim() + ' and friends';
    const cut = Array.from(long).slice(0, 64).join('');
    expect(Array.from(long).length).toBeGreaterThan(64);
    const backend = createMockBackend({ server: 'online' });
    const { user } = renderWithProviders(<JoinServer invite="" relinkFor="survival" />, {
      ipc: async (cmd, args) => {
        const answer = await backend.handle(cmd, args);
        if (cmd === 'join_preview') return { ...(answer as object), name: `  ${long} ` };
        if (cmd === 'instances_list') {
          const listing = answer as { instances: Instance[] };
          return {
            ...listing,
            instances: listing.instances.map((i) => (i.id === 'survival' && i.server ? { ...i, server: { ...i.server, name: cut } } : i)),
          };
        }
        return answer;
      },
    });
    await user.type(await screen.findByRole('textbox', { name: 'Invite link' }), OTHER);
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(await screen.findByRole('button', { name: 'Use this link' })).toBeInTheDocument();
    expect(screen.queryByText(/This link is for/)).not.toBeInTheDocument();
  });

  it('links a plain instance, saying where its old mods go', async () => {
    const { user, calls } = renderApp(<JoinServer invite="" linkFor="asalith-fields" />, { server: 'online' });
    expect(await screen.findByRole('heading', { name: 'Link to a server' })).toBeInTheDocument();
    await user.type(screen.getByRole('textbox', { name: 'Invite link' }), OTHER);
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(
      await screen.findByText(
        "When you press Play, mods the server doesn't use move to the mods-before-link folder. Your worlds and settings stay.",
      ),
    ).toBeInTheDocument();
    // Same game version and loader: nothing to warn about.
    expect(screen.queryByText(/back up your worlds/)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Link' }));
    expect(sent(calls, 'server_link')).toEqual([{ id: 'asalith-fields', link: OTHER }]);
    expect(sent(calls, 'join_add')).toEqual([]);
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'asalith-fields' }));
  });

  it('warns when linking moves the instance to another game version', async () => {
    const { user } = renderApp(<JoinServer invite="" linkFor="creative-flat" />, { server: 'online' });
    await user.type(await screen.findByRole('textbox', { name: 'Invite link' }), OTHER);
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(
      await screen.findByText('This server plays Minecraft 1.20.1 with Fabric. Linking moves this instance to that version, so back up your worlds first.'),
    ).toBeInTheDocument();
  });

  it('warns when a modded instance links to a vanilla server', async () => {
    const { user } = renderApp(<JoinServer invite="" linkFor="asalith-fields" />, { server: 'vanilla' });
    await user.type(await screen.findByRole('textbox', { name: 'Invite link' }), OTHER);
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(
      await screen.findByText('This server plays vanilla Minecraft 1.21.1. Linking moves this instance to that version, so back up your worlds first.'),
    ).toBeInTheDocument();
  });

  it('reads a server that names its loader "vanilla" as vanilla', async () => {
    const backend = createMockBackend({ server: 'vanilla' });
    const { user } = renderWithProviders(<JoinServer invite="" linkFor="creative-flat" />, {
      ipc: async (cmd, args) => {
        const answer = await backend.handle(cmd, args);
        if (cmd !== 'join_preview') return answer;
        const preview = answer as { game: object };
        return { ...preview, game: { ...preview.game, version: '26.3', loader: 'vanilla' } };
      },
    });
    await user.type(await screen.findByRole('textbox', { name: 'Invite link' }), OTHER);
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(await screen.findByRole('button', { name: 'Link' })).toBeInTheDocument();
    expect(screen.queryByText(/back up your worlds/)).not.toBeInTheDocument();
  });

  it('names a server on LATEST in words, never the raw token', async () => {
    const backend = createMockBackend({ server: 'vanilla' });
    renderWithProviders(<JoinServer invite={OTHER} />, {
      ipc: async (cmd, args) => {
        const answer = await backend.handle(cmd, args);
        if (cmd !== 'join_preview') return answer;
        const preview = answer as { game: object };
        return { ...preview, game: { ...preview.game, version: 'LATEST', loader: null } };
      },
    });
    expect(await screen.findByText('Vanilla Minecraft (latest)')).toBeInTheDocument();
    expect(screen.queryByText(/LATEST/)).not.toBeInTheDocument();
  });

  it('warns in words when linking to a server on LATEST', async () => {
    const backend = createMockBackend({ server: 'vanilla' });
    const { user } = renderWithProviders(<JoinServer invite="" linkFor="asalith-fields" />, {
      ipc: async (cmd, args) => {
        const answer = await backend.handle(cmd, args);
        if (cmd !== 'join_preview') return answer;
        const preview = answer as { game: object };
        return { ...preview, game: { ...preview.game, version: 'latest', loader: null } };
      },
    });
    await user.type(await screen.findByRole('textbox', { name: 'Invite link' }), OTHER);
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(
      await screen.findByText('This server plays vanilla Minecraft (latest). Linking moves this instance to that version, so back up your worlds first.'),
    ).toBeInTheDocument();
  });

  it('links even when another instance already joins that server', async () => {
    const { user, calls } = renderApp(<JoinServer invite={MOCK_INVITE} linkFor="asalith-fields" />, { server: 'online' });
    await user.click(await screen.findByRole('button', { name: 'Link' }));
    expect(sent(calls, 'server_link')).toEqual([{ id: 'asalith-fields', link: MOCK_INVITE }]);
  });

  it('join_screen_records_joined_on_add', async () => {
    const { user, calls } = renderApp(<JoinServer invite={OTHER} />, { server: 'online', perland: 'signedIn' });
    await user.click(await screen.findByRole('button', { name: 'Add server' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival-2' }));
    expect(sent(calls, 'your_servers_joined')).toEqual([{ invite: OTHER }]);
  });

  it('join_screen_records_joined_on_existing', async () => {
    const { user, calls } = renderApp(<JoinServer invite={MOCK_INVITE} />, { server: 'online', perland: 'signedIn' });
    await user.click(await screen.findByRole('button', { name: 'Open' }));
    expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' });
    expect(sent(calls, 'your_servers_joined')).toEqual([{ invite: MOCK_INVITE }]);
  });

  it('records joined with the canonical link on relink and link', async () => {
    const relink = renderApp(<JoinServer invite="" relinkFor="survival" />, { server: 'vanilla', perland: 'signedIn' });
    await relink.user.type(await screen.findByRole('textbox', { name: 'Invite link' }), `https://PLAY.example.com/${'B'.repeat(43)}/`);
    await relink.user.click(screen.getByRole('button', { name: 'Check link' }));
    await relink.user.click(await screen.findByRole('button', { name: 'Use this link' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' }));
    expect(sent(relink.calls, 'your_servers_joined')).toEqual([{ invite: OTHER }]);
    relink.unmount();

    const link = renderApp(<JoinServer invite="" linkFor="asalith-fields" />, { server: 'online', perland: 'signedIn' });
    await link.user.type(await screen.findByRole('textbox', { name: 'Invite link' }), OTHER);
    await link.user.click(screen.getByRole('button', { name: 'Check link' }));
    await link.user.click(await screen.findByRole('button', { name: 'Link' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'asalith-fields' }));
    expect(sent(link.calls, 'your_servers_joined')).toEqual([{ invite: OTHER }]);
  });

  it('a failed joined call is never shown', async () => {
    const backend = createMockBackend({ server: 'online', perland: 'signedIn' });
    const calls: [string, unknown][] = [];
    const { user } = renderWithProviders(<JoinServer invite={OTHER} />, {
      ipc: (cmd, args) => {
        calls.push([cmd, args]);
        return cmd === 'your_servers_joined'
          ? Promise.reject({ category: 'network', code: 'host_unreachable', message: 'x', fix: null, detail: null, params: { host: 'play.example.com' } })
          : backend.handle(cmd, args);
      },
    });
    await user.click(await screen.findByRole('button', { name: 'Add server' }));
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival-2' }));
    expect(sent(calls, 'your_servers_joined')).toHaveLength(1);
    expect(toastsSnapshot()).toEqual([]);
  });

  it('locks the primary button while the add is in flight, so a double click adds once', async () => {
    const backend = createMockBackend({ server: 'online' });
    const calls: [string, unknown][] = [];
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => (release = resolve));
    const { user } = renderWithProviders(<JoinServer invite={OTHER} />, {
      ipc: async (cmd, args) => {
        calls.push([cmd, args]);
        if (cmd === 'join_add') await held;
        return backend.handle(cmd, args);
      },
    });
    const add = await screen.findByRole('button', { name: 'Add server' });
    await user.dblClick(add);
    expect(screen.getByRole('button', { name: 'Adding…' })).toBeDisabled();
    expect(sent(calls, 'join_add')).toEqual([{ link: OTHER }]);
    await act(async () => release());
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival-2' }));
    expect(sent(calls, 'join_add')).toHaveLength(1);
  });
});
