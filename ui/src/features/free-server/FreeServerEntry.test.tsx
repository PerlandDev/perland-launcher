import { screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FreeScenario } from '../../ipc/mockFree';
import { renderApp, sent } from '../../test/backend';
import { ServersScreen } from '../servers-list/ServersScreen';

const ENTRY = 'Create a server for friends';
const yours = () => screen.findByRole('region', { name: 'Your servers' });

describe('FreeServerEntry', () => {
  it('shows the entry in Your servers while the player may create', async () => {
    renderApp(<ServersScreen />, { perland: 'signedIn', free: 'none' });
    const section = await yours();
    expect(await within(section).findByRole('button', { name: ENTRY })).toBeInTheDocument();
  });

  it('shows Your servers for the entry even with no servers', async () => {
    renderApp(<ServersScreen />, { perland: 'empty', free: 'none' });
    const section = await yours();
    expect(await within(section).findByRole('button', { name: ENTRY })).toBeInTheDocument();
  });

  it('entry_reasons', async () => {
    const cases: [FreeScenario, string][] = [
      [
        'emailRequired',
        'Free servers need a verified email on your Perland account. Add one, then try again.',
      ],
      ['exists', 'Another account with your email already has a free server.'],
      ['rateLimited', 'You can create another server in 2 hours.'],
      ['full', 'Free servers are full right now. Try again later.'],
    ];
    for (const [free, line] of cases) {
      const { unmount } = renderApp(<ServersScreen />, {
        perland: 'signedIn',
        free,
      });
      const section = await yours();
      expect(await within(section).findByText(line)).toBeInTheDocument();
      expect(within(section).queryByRole('button', { name: ENTRY })).toBeNull();
      unmount();
    }
  });

  it('email required offers Add an email, panel or not, and the email step opens in the launcher', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'github', free: 'emailRequired', panel: false });
    const section = await yours();
    expect(within(section).queryByText(/Discord/)).toBeNull();
    await user.click(await within(section).findByRole('button', { name: 'Add an email' }));
    const dialog = await screen.findByRole('dialog', { name: 'Add an email' });
    expect(within(dialog).getByRole('textbox', { name: 'Email' })).toBeInTheDocument();
    expect(sent(calls, 'perland_open_account')).toHaveLength(0);
  });

  it('a verified email closes the step and opens the way to create', async () => {
    const { user, calls } = renderApp(<ServersScreen />, { perland: 'github', free: 'emailRequired' });
    const section = await yours();
    await user.click(await within(section).findByRole('button', { name: 'Add an email' }));
    const dialog = await screen.findByRole('dialog', { name: 'Add an email' });
    await user.type(within(dialog).getByRole('textbox', { name: 'Email' }), 'a@b.co');
    await user.click(within(dialog).getByRole('button', { name: 'Send code' }));
    await user.type(await within(dialog).findByRole('textbox', { name: 'Code' }), '123456');
    const statusBefore = sent(calls, 'perland_status').length;
    await user.click(within(dialog).getByRole('button', { name: 'Verify' }));
    expect(await within(section).findByRole('button', { name: ENTRY })).toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: 'Add an email' })).toBeNull();
    await vi.waitFor(() => expect(sent(calls, 'perland_status').length).toBeGreaterThan(statusBefore));
  });

  it('closing the step mid-way and opening it again starts at the email field', async () => {
    const { user } = renderApp(<ServersScreen />, { perland: 'github', free: 'emailRequired' });
    const section = await yours();
    await user.click(await within(section).findByRole('button', { name: 'Add an email' }));
    let dialog = await screen.findByRole('dialog', { name: 'Add an email' });
    await user.type(within(dialog).getByRole('textbox', { name: 'Email' }), 'a@b.co');
    await user.click(within(dialog).getByRole('button', { name: 'Send code' }));
    await within(dialog).findByRole('textbox', { name: 'Code' });
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog', { name: 'Add an email' })).toBeNull();
    await user.click(within(section).getByRole('button', { name: 'Add an email' }));
    dialog = await screen.findByRole('dialog', { name: 'Add an email' });
    expect(within(dialog).getByRole('textbox', { name: 'Email' })).toHaveValue('');
    expect(within(dialog).queryByRole('textbox', { name: 'Code' })).toBeNull();
  });

  it('nothing once the player has a server', async () => {
    const { calls } = renderApp(<ServersScreen />, {
      perland: 'signedIn',
      free: 'has',
    });
    await yours();
    await vi.waitFor(() => expect(sent(calls, 'free_server').length).toBeGreaterThan(0));
    expect(screen.queryByRole('button', { name: ENTRY })).toBeNull();
    expect(screen.queryByText('Another account with your email already has a free server.')).toBeNull();
  });

  it('nothing when free servers are off or signed out', async () => {
    const off = renderApp(<ServersScreen />, {
      perland: 'signedIn',
      free: 'off',
    });
    await yours();
    await vi.waitFor(() => expect(sent(off.calls, 'free_server').length).toBeGreaterThan(0));
    expect(screen.queryByRole('button', { name: ENTRY })).toBeNull();
    off.unmount();
    const out = renderApp(<ServersScreen />, { perland: 'signedOut' });
    expect(await screen.findByText('Sign in with Perland to keep your servers here.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: ENTRY })).toBeNull();
    expect(sent(out.calls, 'free_server')).toEqual([]);
  });
});
