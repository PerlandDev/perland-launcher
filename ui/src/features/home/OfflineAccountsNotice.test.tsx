import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { renderApp, sent } from '../../test/backend';
import { Home } from './Home';
import { OfflineAccountsNotice } from './OfflineAccountsNotice';

const NOTICE = 'Offline accounts are gone. Perland Launcher now needs a Microsoft account that owns Minecraft: Java Edition.';

describe('OfflineAccountsNotice', () => {
  it('shows while the core asks for it, with the sign-in', async () => {
    renderApp(<OfflineAccountsNotice />, { beta: true });
    expect(await screen.findByText(NOTICE)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Sign in with Microsoft' })).toBeInTheDocument();
    // The notice already says why; the sign-in under it doesn't repeat it.
    expect(screen.queryByText('You need a Microsoft account that owns Minecraft: Java Edition.')).toBeNull();
  });

  it('shows nothing when not asked for', async () => {
    const { calls } = renderApp(<OfflineAccountsNotice />);
    await vi.waitFor(() => expect(sent(calls, 'account_ready').length).toBeGreaterThan(0));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.queryByText(NOTICE)).toBeNull();
  });

  it('Dismiss tells the core and hides it', async () => {
    const { user, calls } = renderApp(<OfflineAccountsNotice />, { beta: true });
    await user.click(await screen.findByRole('button', { name: 'Dismiss' }));
    expect(sent(calls, 'account_dismiss_offline_notice')).toHaveLength(1);
    await vi.waitFor(() => expect(screen.queryByText(NOTICE)).toBeNull());
  });

  it('shows on Home', async () => {
    renderApp(<Home id={null} />, { beta: true });
    expect(await screen.findByText(NOTICE)).toBeInTheDocument();
  });
});
