import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend, type MockOptions } from '../../ipc/mock';
import { MOCK_TEAM_SERVER, MOCK_TEAM_TENANT } from '../../ipc/mockManage';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ManagePlayers } from './ManagePlayers';

const ref = { tenantId: MOCK_TEAM_TENANT, stack: MOCK_TEAM_SERVER };
const show = (manage: MockOptions['manage'], onOpenSettings = () => {}) =>
  renderApp(<ManagePlayers {...ref} onOpenSettings={onOpenSettings} />, { perland: 'signedIn', manage });
const section = (name: string) => screen.findByRole('region', { name });

describe('ManagePlayers', () => {
  it('adds a name to the whitelist and says what happened to it', async () => {
    const { user, calls } = show('running');
    const list = await section('Whitelist');
    await within(list).findByText('Steve');
    await user.type(within(list).getByRole('textbox', { name: 'Add to the whitelist' }), 'Alex');
    await user.click(within(list).getByRole('button', { name: 'Add' }));
    await vi.waitFor(() =>
      expect(sent(calls, 'manage_players_change')).toEqual([{ ...ref, change: { list: 'whitelist', add: true, names: ['Alex'] } }]),
    );
    expect(await within(list).findByText('Alex is already on the list.')).toBeInTheDocument();
    await user.type(within(list).getByRole('textbox', { name: 'Add to the whitelist' }), 'Kai');
    await user.click(within(list).getByRole('button', { name: 'Add' }));
    expect(await within(list).findByText('Kai was added.')).toBeInTheDocument();
  });

  it('makes an admin and takes it away with plain words', async () => {
    const { user, calls } = show('running');
    const list = await section('Admins');
    await user.type(within(list).getByRole('textbox', { name: 'Make admin' }), 'Kai');
    await user.click(within(list).getByRole('button', { name: 'Make admin' }));
    expect(await within(list).findByText('Kai is an admin now.')).toBeInTheDocument();
    await user.click(within(list).getByRole('button', { name: 'Remove admin: Steve' }));
    await vi.waitFor(() =>
      expect(sent(calls, 'manage_players_change')).toContainEqual({ ...ref, change: { list: 'admins', add: false, names: ['Steve'] } }),
    );
  });

  it('a ban asks first', async () => {
    const { user, calls } = show('running');
    const list = await section('Bans');
    await user.type(within(list).getByRole('textbox', { name: 'Ban a player' }), 'Notch');
    await user.click(within(list).getByRole('button', { name: 'Ban' }));
    const dialog = screen.getByRole('dialog', { name: 'Ban Notch?' });
    expect(sent(calls, 'manage_players_change')).toEqual([]);
    await user.click(within(dialog).getByRole('button', { name: 'Ban' }));
    await vi.waitFor(() =>
      expect(sent(calls, 'manage_players_change')).toEqual([{ ...ref, change: { list: 'bans', add: true, names: ['Notch'] } }]),
    );
    expect(await within(list).findByText('Notch is banned.')).toBeInTheDocument();
  });

  it('kicks without asking', async () => {
    const { user, calls } = show('running');
    const online = await section('Online now');
    await user.click(await within(online).findByRole('button', { name: 'Kick Steve' }));
    await vi.waitFor(() =>
      expect(sent(calls, 'manage_players_change')).toEqual([{ ...ref, change: { list: 'online', add: false, names: ['Steve'] } }]),
    );
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('a stopped server keeps the lists but bans need it running', async () => {
    show('stopped');
    const bans = await section('Bans');
    expect(await within(bans).findByText('Griefer')).toBeInTheDocument();
    expect(within(bans).getByText('Start the server to change bans.')).toBeInTheDocument();
    expect(within(bans).queryByRole('textbox')).toBeNull();
    expect(within(await section('Whitelist')).getByRole('textbox', { name: 'Add to the whitelist' })).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Online now' })).toBeNull();
  });

  it('links to the whitelist setting', async () => {
    const open = vi.fn();
    const { user } = show('running', open);
    await user.click(await screen.findByRole('button', { name: 'Whitelist is off — turn it on in Settings' }));
    expect(open).toHaveBeenCalled();
  });

  it('lists that are not readable while it starts are worded softly', async () => {
    const backend = createMockBackend({ perland: 'signedIn', manage: 'starting' });
    renderWithProviders(<ManagePlayers {...ref} onOpenSettings={() => {}} />, {
      ipc: async (cmd, args) => {
        const answer = await backend.handle(cmd, args);
        return cmd === 'manage_players' ? { ...(answer as object), unreadable: true } : answer;
      },
    });
    expect(await screen.findByText('The lists show once the server has started.')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
