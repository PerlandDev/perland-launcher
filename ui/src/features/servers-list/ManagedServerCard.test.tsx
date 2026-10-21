import { screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import i18n from '../../i18n';
import type { ManagedServer } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { renderApp } from '../../test/backend';
import { ManagedServerCard } from './ManagedServerCard';

const server = (over: Partial<ManagedServer> = {}): ManagedServer => ({
  tenantId: 't-1',
  teamName: 'Builders',
  stack: 'team-survival',
  name: 'Team survival',
  state: 'running',
  address: 'play.example.com:25600',
  free: false,
  ...over,
});

describe('ManagedServerCard', () => {
  it('shows the name, team, state in words and the address, and Manage opens the screen', async () => {
    const { user } = renderApp(<ManagedServerCard server={server()} />);
    const card = screen.getByRole('article', { name: 'Team survival' });
    expect(within(card).getByText('Builders')).toBeInTheDocument();
    expect(within(card).getByText('Running')).toBeInTheDocument();
    expect(within(card).getByText('Join from Minecraft: play.example.com:25600')).toBeInTheDocument();
    expect(within(card).getByRole('button', { name: 'Copy server address' })).toBeInTheDocument();
    expect(within(card).queryByRole('button', { name: 'Play' })).toBeNull();
    await user.click(within(card).getByRole('button', { name: 'Manage' }));
    expect(routeSnapshot()).toEqual({ name: 'manage', tenantId: 't-1', stack: 'team-survival' });
  });

  it('says each state in words, and leaves the address out when there is none', () => {
    const { rerender } = renderApp(<ManagedServerCard server={server({ state: 'asleep', address: null })} />);
    expect(screen.getByText('Asleep · wakes when someone joins')).toBeInTheDocument();
    expect(screen.queryByText(/^Join from Minecraft/)).toBeNull();
    rerender(<ManagedServerCard server={server({ state: 'stopped' })} />);
    expect(screen.getByText('Stopped')).toBeInTheDocument();
    rerender(<ManagedServerCard server={server({ state: 'starting' })} />);
    expect(screen.getByText('Starting…')).toBeInTheDocument();
  });

  it('speaks Spanish', async () => {
    await i18n.changeLanguage('es');
    renderApp(<ManagedServerCard server={server()} />);
    expect(screen.getByRole('button', { name: 'Administrar' })).toBeInTheDocument();
  });
});
