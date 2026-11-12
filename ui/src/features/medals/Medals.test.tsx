import { screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { renderApp } from '../../test/backend';
import { Medals } from './Medals';

const medal = (name: string) => screen.getByText(name, { selector: '.medal p' }).closest('li') as HTMLElement;
const earned = () => screen.getByRole('region', { name: /Medals earned/ });
const toEarn = () => screen.getByRole('region', { name: /Medals to earn/ });

describe('Medals', () => {
  it('shows the medals earned on this computer with their date, and keeps them local while signed out', async () => {
    renderApp(<Medals />, { playtime: { 'asalith-fields': 12 * 3600 } });
    await waitFor(() => expect(medal('Time played')).toHaveTextContent('Iron · 12 h'));
    const time = medal('Time played');
    expect(earned()).toContainElement(time);
    expect(time).toHaveTextContent('Next: Gold at 4 days 4 h');
    expect(time).toHaveTextContent('Earned on Nov 7, 2026');
    expect(within(time).getByRole('progressbar', { name: '12 h of 4 days 4 h' })).toBeInTheDocument();
    expect(earned()).toContainElement(medal('First game'));
    expect(toEarn()).toContainElement(medal('Dragon slayer'));
    expect(screen.getByText(/saved on this computer only/)).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: 'Sign in' })).toHaveLength(2);
  });

  it('shows every step of a ladder, the earned ones in their metal', async () => {
    renderApp(<Medals />, { playtime: { 'asalith-fields': 12 * 3600 } });
    await waitFor(() => expect(medal('Time played')).toHaveTextContent('Iron · 12 h'));
    const steps = within(medal('Time played')).getByRole('list', { name: 'Tiers' });
    const items = within(steps).getAllByRole('listitem');
    expect(items.map((i) => i.textContent)).toEqual(['1 hCopper, Earned', '10 hIron, Earned', '100 hGold, Locked', '500 hDiamond, Locked']);
    expect(items.map((i) => i.hasAttribute('data-locked'))).toEqual([false, false, true, true]);
  });

  it('says the medals are saved to the account once signed in', async () => {
    renderApp(<Medals />, { perland: 'signedIn' });
    expect(await screen.findByText(/saved to your Perland account/)).toBeInTheDocument();
  });

  it('earns the Perland medals from the linked stats and lists the servers', async () => {
    renderApp(<Medals />, { perland: 'linked' });
    await waitFor(() => expect(medal('Dragon slayer')).toHaveTextContent('Diamond · Earned'));
    expect(medal('Wither summoner')).toHaveTextContent('Locked: Summon the Wither');
    // 12 + 7 + 3 IDs; the four story starts repeat, and count once.
    expect(medal('Achiever')).toHaveTextContent('Copper · 15');
    expect(medal('Server regular')).toHaveTextContent('Iron · 1 day 6 h');
    expect(medal('Traveler')).toHaveTextContent('Gold · 3');
    expect(await screen.findByRole('button', { name: 'See your achievements on My world' })).toHaveTextContent('12 achievements');
    // Linked: nothing left to sign in to or link.
    expect(screen.queryByRole('button', { name: 'Sign in' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Link accounts' })).toBeNull();
  });

  it('opens a server’s achievements', async () => {
    const { user } = renderApp(<Medals />, { perland: 'linked' });
    await user.click(await screen.findByRole('button', { name: 'See your achievements on My world' }));
    const dialog = screen.getByRole('dialog', { name: 'Achievements on My world' });
    const end = within(dialog).getByRole('region', { name: 'The End' });
    expect(end).toHaveTextContent('2 of 9');
    expect(within(end).getByText('Free the End').closest('li')).toHaveAttribute('data-earned');
    expect(within(end).getByText("Sky's the Limit").closest('li')).not.toHaveAttribute('data-earned');
  });
});
