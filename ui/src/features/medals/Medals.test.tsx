import { act, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { renderApp, sent } from '../../test/backend';
import { Medals } from './Medals';

const medal = (name: string) => screen.getByText(name, { selector: '.medal p' }).closest('li') as HTMLElement;

describe('Medals', () => {
  it('shows only the time medal without Perland stats', async () => {
    const { calls } = renderApp(<Medals />, { playtime: { 'asalith-fields': 12 * 3600 } });
    await waitFor(() => expect(medal('Time played')).toHaveTextContent('Iron · 12 h'));
    // The status has answered too, so no server medal is still on its way.
    await waitFor(() => expect(sent(calls, 'perland_status')).toHaveLength(1));
    await act(async () => {});
    const time = medal('Time played');
    expect(time).toHaveTextContent('Next: Gold at 4 days 4 h');
    expect(within(time).getByRole('progressbar', { name: '12 h of 4 days 4 h' })).toHaveAttribute('aria-valuenow', '12');
    expect(screen.queryByText('Dragon slayer')).toBeNull();
    expect(screen.getByText('1 of 1 earned')).toBeInTheDocument();
  });

  it('shows the server medals locked, with a line on linking, while not linked', async () => {
    renderApp(<Medals />, { perland: 'signedIn' });
    expect(await screen.findByText('Link your Minecraft account to earn the server medals.')).toBeInTheDocument();
    const dragon = medal('Dragon slayer');
    expect(dragon).toHaveTextContent('Locked: Defeat the Ender Dragon');
    expect(screen.queryByText('Achievements by server')).toBeNull();
  });

  it('earns medals from the linked stats and lists the servers', async () => {
    renderApp(<Medals />, { perland: 'linked' });
    await screen.findByText('Achievements by server');
    expect(screen.queryByText(/Link your Minecraft account/)).toBeNull();
    expect(medal('Dragon slayer')).toHaveTextContent('Diamond · Earned');
    expect(medal('Wither summoner')).toHaveTextContent('Locked: Summon the Wither');
    // 12 + 7 + 3 IDs; the four story starts repeat, and count once.
    expect(medal('Achiever')).toHaveTextContent('Copper · 15');
    expect(medal('Server regular')).toHaveTextContent('Iron · 1 day 6 h');
    expect(medal('Traveler')).toHaveTextContent('Gold · 3');
    expect(screen.getByRole('button', { name: 'See your achievements on My world' })).toHaveTextContent('12 achievements');
  });

  it('opens a server’s achievements from Accounts', async () => {
    const { user } = renderApp(<Medals />, { perland: 'linked' });
    await user.click(await screen.findByRole('button', { name: 'See your achievements on My world' }));
    const dialog = screen.getByRole('dialog', { name: 'Achievements on My world' });
    const end = within(dialog).getByRole('region', { name: 'The End' });
    expect(end).toHaveTextContent('2 of 9');
    expect(within(end).getByText('Free the End').closest('li')).toHaveAttribute('data-earned');
    expect(within(end).getByText('Sky\'s the Limit').closest('li')).not.toHaveAttribute('data-earned');
    expect(within(end).getByText('Sky\'s the Limit').closest('li')).toHaveTextContent('Not yet:');
  });
});
