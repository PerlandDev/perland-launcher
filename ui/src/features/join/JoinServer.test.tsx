import { act, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import { MOCK_INVITE } from '../../ipc/mockServers';
import { routeSnapshot } from '../../state/route';
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
