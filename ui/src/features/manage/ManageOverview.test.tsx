import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend, type MockOptions } from '../../ipc/mock';
import { MOCK_TEAM_SERVER, MOCK_TEAM_TENANT } from '../../ipc/mockManage';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ManageOverview } from './ManageOverview';

const ref = { tenantId: MOCK_TEAM_TENANT, stack: MOCK_TEAM_SERVER };
const show = (manage: MockOptions['manage']) => renderApp(<ManageOverview {...ref} />, { perland: 'signedIn', manage });

describe('ManageOverview', () => {
  it('running says who plays, and Stop asks before it stops anything', async () => {
    const { user, calls } = show('running');
    expect(await screen.findByText('Running · 3 playing')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Restart' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Stop' }));
    const dialog = screen.getByRole('dialog', { name: 'Stop Team survival?' });
    expect(within(dialog).getByText('Everyone playing is sent out.')).toBeInTheDocument();
    expect(sent(calls, 'manage_action')).toEqual([]);
    await user.click(within(dialog).getByRole('button', { name: 'Stop' }));
    await vi.waitFor(() => expect(sent(calls, 'manage_action')).toEqual([{ ...ref, action: 'stop' }]));
  });

  it('restart asks too', async () => {
    const { user, calls } = show('running');
    await user.click(await screen.findByRole('button', { name: 'Restart' }));
    const dialog = screen.getByRole('dialog', { name: 'Restart Team survival?' });
    await user.click(within(dialog).getByRole('button', { name: 'Restart' }));
    await vi.waitFor(() => expect(sent(calls, 'manage_action')).toEqual([{ ...ref, action: 'restart' }]));
  });

  it('asleep says it wakes when someone joins, and Start wakes it at once', async () => {
    const { user, calls } = show('asleep');
    expect(await screen.findByText('Asleep · wakes when someone joins')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Start' }));
    await vi.waitFor(() => expect(sent(calls, 'manage_action')).toEqual([{ ...ref, action: 'start' }]));
    expect(await screen.findByText('Starting…')).toBeInTheDocument();
    // Nothing to press while it changes.
    expect(screen.queryByRole('button', { name: 'Start' })).toBeNull();
  });

  it('stopped offers Start and no Stop', async () => {
    show('stopped');
    expect(await screen.findByText('Stopped')).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Start' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Stop' })).toBeNull();
  });

  it('crashed says why, in plain words, and offers Start again', async () => {
    const { calls } = show('crashed');
    expect(await screen.findByText('Crashed')).toBeInTheDocument();
    expect(screen.getByText('The server ran out of memory and stopped.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start again' })).toBeInTheDocument();
    expect(sent(calls, 'manage_diagnosis')).toHaveLength(1);
  });

  it('a running server never asks for the diagnosis', async () => {
    const { calls } = show('running');
    await screen.findByText('Running · 3 playing');
    expect(sent(calls, 'manage_diagnosis')).toEqual([]);
  });

  it('shows the address to copy', async () => {
    show('running');
    expect(await screen.findByText('play.example.com:25600')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Copy server address' })).toBeInTheDocument();
  });

  it('says when saved settings wait for a restart', async () => {
    const backend = createMockBackend({ perland: 'signedIn', manage: 'running' });
    renderWithProviders(<ManageOverview {...ref} />, {
      ipc: async (cmd, args) => {
        const answer = await backend.handle(cmd, args);
        return cmd === 'manage_detail' ? { ...(answer as object), pendingRestart: true } : answer;
      },
    });
    expect(await screen.findByText('Restart to apply your saved settings.')).toBeInTheDocument();
  });

  it('More in Perland opens the panel at this server', async () => {
    const { user, calls } = show('running');
    await user.click(await screen.findByRole('button', { name: 'More in Perland' }));
    await vi.waitFor(() => expect(sent(calls, 'manage_open_panel')).toEqual([ref]));
  });
});
