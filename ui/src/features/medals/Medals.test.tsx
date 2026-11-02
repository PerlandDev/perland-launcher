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
    expect(screen.queryByText('Achievements on Perland servers')).toBeNull();
  });

  it('earns medals from the linked stats and lists the servers', async () => {
    renderApp(<Medals />, { perland: 'linked' });
    await screen.findByText('Achievements on Perland servers');
    expect(screen.queryByText(/Link your Minecraft account/)).toBeNull();
    expect(medal('Dragon slayer')).toHaveTextContent('Diamond · Earned');
    expect(medal('Wither summoner')).toHaveTextContent('Locked: Summon the Wither');
    // 12 + 7 + 3 IDs; the four story starts repeat, and count once.
    expect(medal('Achiever')).toHaveTextContent('Copper · 15');
    expect(medal('Server regular')).toHaveTextContent('Iron · 1 day 6 h');
    expect(medal('Traveler')).toHaveTextContent('Gold · 3');
    expect(screen.getByRole('button', { name: 'See your achievements on My world' })).toHaveTextContent('12 achievements');
  });

  it('opens a server’s achievements', async () => {
    const { user } = renderApp(<Medals />, { perland: 'linked' });
    await user.click(await screen.findByRole('button', { name: 'See your achievements on My world' }));
    const dialog = screen.getByRole('dialog', { name: 'Achievements on My world' });
    const end = within(dialog).getByRole('region', { name: 'The End' });
    expect(end).toHaveTextContent('2 of 9');
    expect(within(end).getByText('Free the End').closest('li')).toHaveAttribute('data-earned');
    expect(within(end).getByText('Sky\'s the Limit').closest('li')).not.toHaveAttribute('data-earned');
    expect(within(end).getByText('Sky\'s the Limit').closest('li')).toHaveTextContent('Not yet:');
  });

  it('shows every step of a ladder, the earned ones in their metal', async () => {
    renderApp(<Medals />, { playtime: { 'asalith-fields': 12 * 3600 } });
    await waitFor(() => expect(medal('Time played')).toHaveTextContent('Iron · 12 h'));
    const steps = within(medal('Time played')).getByRole('list', { name: 'Tiers' });
    const items = within(steps).getAllByRole('listitem');
    expect(items.map((i) => i.textContent)).toEqual([
      '1 hCopper, Earned',
      '10 hIron, Earned',
      '100 hGold, Locked',
      '500 hDiamond, Locked',
    ]);
    expect(items.map((i) => i.hasAttribute('data-locked'))).toEqual([false, false, true, true]);
  });

  it('says which medals come from Perland servers', async () => {
    renderApp(<Medals />);
    expect(
      await screen.findByText('Time played counts everything you play on this computer. The others are earned on servers made on Perland.'),
    ).toBeInTheDocument();
  });

  it('says the server list is only servers made on Perland', async () => {
    renderApp(<Medals />, { perland: 'linked' });
    expect(await screen.findByRole('heading', { name: 'Achievements on Perland servers' })).toBeInTheDocument();
    expect(screen.getByText(/servers made on Perland\. Your single-player worlds don't count here\./)).toBeInTheDocument();
  });
});