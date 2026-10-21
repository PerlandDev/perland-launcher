import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MOCK_TEAM_SERVER, MOCK_TEAM_TENANT } from '../../ipc/mockManage';
import { routeSnapshot } from '../../state/route';
import { renderApp } from '../../test/backend';
import { ManageScreen } from './ManageScreen';

const ref = { tenantId: MOCK_TEAM_TENANT, stack: MOCK_TEAM_SERVER };

describe('ManageScreen', () => {
  it('names the server, starts on Players and switches sections', async () => {
    const { user } = renderApp(<ManageScreen {...ref} />, { perland: 'signedIn', manage: 'running' });
    expect(screen.getByRole('heading', { level: 1, name: 'Team survival' })).toBeInTheDocument();
    expect(await screen.findByRole('region', { name: 'Whitelist' })).toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: 'Settings' }));
    expect(await screen.findByRole('combobox', { name: 'Game mode' })).toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: 'Console' }));
    expect(await screen.findByRole('log', { name: 'Server console' })).toBeInTheDocument();
  });

  it('the whitelist link opens Settings', async () => {
    const { user } = renderApp(<ManageScreen {...ref} />, { perland: 'signedIn', manage: 'running' });
    await user.click(await screen.findByRole('button', { name: 'Whitelist is off — turn it on in Settings' }));
    expect(await screen.findByRole('switch', { name: 'Whitelist' })).toBeInTheDocument();
  });

  it('Back goes to the servers', async () => {
    const { user } = renderApp(<ManageScreen {...ref} />, { perland: 'signedIn', manage: 'running' });
    await user.click(screen.getByRole('button', { name: 'Back' }));
    expect(routeSnapshot()).toEqual({ name: 'servers' });
  });
});
