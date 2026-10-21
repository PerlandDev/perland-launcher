import { act, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend, type MockOptions } from '../../ipc/mock';
import { MOCK_TEAM_SERVER, MOCK_TEAM_TENANT } from '../../ipc/mockManage';
import { handleEvent } from '../../state/handleEvent';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ManageConsole } from './ManageConsole';

const ref = { tenantId: MOCK_TEAM_TENANT, stack: MOCK_TEAM_SERVER };
const show = (manage: MockOptions['manage'] = 'running') => renderApp(<ManageConsole {...ref} />, { perland: 'signedIn', manage });

describe('ManageConsole', () => {
  it('opens the live log on mount and closes it on unmount', async () => {
    const { calls, unmount } = show();
    await vi.waitFor(() => expect(sent(calls, 'manage_console_open')).toEqual([ref]));
    unmount();
    await vi.waitFor(() => expect(sent(calls, 'manage_console_close')).toEqual([ref]));
  });

  it('a close before the open answered closes again once it has', async () => {
    const backend = createMockBackend({ perland: 'signedIn', manage: 'running' });
    const calls: [string, unknown][] = [];
    let opened: (() => void) | null = null;
    const { unmount } = renderWithProviders(<ManageConsole {...ref} />, {
      ipc: (cmd, args) => {
        calls.push([cmd, args]);
        if (cmd === 'manage_console_open') return new Promise<null>((resolve) => (opened = () => resolve(null)));
        return backend.handle(cmd, args);
      },
    });
    await vi.waitFor(() => expect(opened).not.toBeNull());
    unmount();
    expect(sent(calls, 'manage_console_close')).toEqual([ref]);
    act(() => opened?.());
    await vi.waitFor(() => expect(sent(calls, 'manage_console_close')).toEqual([ref, ref]));
  });

  it('a late close never stops a newer open of the same server', async () => {
    const backend = createMockBackend({ perland: 'signedIn', manage: 'running' });
    const calls: [string, unknown][] = [];
    const pending: (() => void)[] = [];
    const ipc = (cmd: string, args?: unknown) => {
      calls.push([cmd, args]);
      if (cmd === 'manage_console_open') return new Promise<null>((resolve) => pending.push(() => resolve(null)));
      return backend.handle(cmd, args);
    };
    const first = renderWithProviders(<ManageConsole {...ref} />, { ipc });
    await vi.waitFor(() => expect(pending).toHaveLength(1));
    first.unmount();
    const second = renderWithProviders(<ManageConsole {...ref} />, { ipc });
    await vi.waitFor(() => expect(pending).toHaveLength(2));
    // The first open settles after the second was sent: no close may follow it.
    act(() => pending[0]?.());
    act(() => pending[1]?.());
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(sent(calls, 'manage_console_close')).toEqual([ref]);
    const log = await second.findByRole('log', { name: 'Server console' });
    act(() => handleEvent({ type: 'server_log', ...ref, lines: ['still live'] }));
    expect(within(log).getByText('still live')).toBeInTheDocument();
  });

  it('shows the lines the server sends', async () => {
    show();
    const log = await screen.findByRole('log', { name: 'Server console' });
    act(() => handleEvent({ type: 'server_log', ...ref, lines: ['[12:00:00] [Server thread/INFO]: Hello there'] }));
    expect(within(log).getByText('[12:00:00] [Server thread/INFO]: Hello there')).toBeInTheDocument();
    // Another server's lines stay out.
    act(() => handleEvent({ type: 'server_log', tenantId: ref.tenantId, stack: 'Other', lines: ['not mine'] }));
    expect(within(log).queryByText('not mine')).toBeNull();
  });

  it('a reopen shows the tail once, not twice', async () => {
    const first = show();
    let log = await screen.findByRole('log', { name: 'Server console' });
    await vi.waitFor(() => expect(sent(first.calls, 'manage_console_open')).toHaveLength(1));
    // The open sends the last lines; they arrive as events.
    act(() => handleEvent({ type: 'server_log', ...ref, lines: ['tail one', 'tail two'] }));
    expect(within(log).getByText('tail one')).toBeInTheDocument();
    first.unmount();
    const second = show();
    await vi.waitFor(() => expect(sent(second.calls, 'manage_console_open')).toHaveLength(1));
    log = await screen.findByRole('log', { name: 'Server console' });
    // The new open sends the same tail again.
    act(() => handleEvent({ type: 'server_log', ...ref, lines: ['tail one', 'tail two'] }));
    expect(within(log).getAllByText('tail one')).toHaveLength(1);
    expect(within(log).getAllByText('tail two')).toHaveLength(1);
  });

  it('a stop asks first, then sends it again with the confirmation', async () => {
    const { user, calls } = show();
    await user.type(await screen.findByRole('textbox', { name: 'Command' }), 'stop');
    await user.click(screen.getByRole('button', { name: 'Send' }));
    const dialog = await screen.findByRole('dialog', { name: 'This stops the server. Run it?' });
    await user.click(within(dialog).getByRole('button', { name: 'Run it' }));
    await vi.waitFor(() =>
      expect(sent(calls, 'manage_console')).toEqual([
        { ...ref, command: 'stop', confirm: false },
        { ...ref, command: 'stop', confirm: true },
      ]),
    );
    expect(await screen.findByText('Stopping the server')).toBeInTheDocument();
  });

  it('shows the answer to a command', async () => {
    const { user } = show();
    await user.type(await screen.findByRole('textbox', { name: 'Command' }), 'say hi');
    await user.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Ran: say hi')).toBeInTheDocument();
  });

  it('offers to reconnect when the log stops', async () => {
    const { user, calls } = show();
    await screen.findByRole('log', { name: 'Server console' });
    await vi.waitFor(() => expect(sent(calls, 'manage_console_open')).toHaveLength(1));
    act(() => handleEvent({ type: 'server_log_ended', ...ref }));
    await user.click(await screen.findByRole('button', { name: 'Reconnect' }));
    await vi.waitFor(() => expect(sent(calls, 'manage_console_open')).toHaveLength(2));
  });

  it('a stopped server has no console', async () => {
    const { calls } = show('stopped');
    expect(await screen.findByText('Start the server to use the console.')).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: 'Command' })).toBeNull();
    expect(sent(calls, 'manage_console_open')).toEqual([]);
  });
});
