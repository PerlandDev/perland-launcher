import { screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { ServerStats } from '../../ipc/schema';
import { renderWithProviders } from '../../test/render';
import { AchievementsDialog } from './AchievementsDialog';

const server = (advancementIds: string[]): ServerStats => ({
  token: 'T'.repeat(43),
  instance: null,
  name: 'Asalith',
  playtimeSeconds: 0,
  advancements: advancementIds.length,
  advancementIds,
});

describe('AchievementsDialog', () => {
  it('groups by tab, then mods, and notes made-up mod names', () => {
    renderWithProviders(
      <AchievementsDialog server={server(['create:andesite_alloy', 'minecraft:story/root', 'minecraft:story/mine_diamond'])} open onClose={() => {}} />,
    );
    const dialog = screen.getByRole('dialog', { name: 'Achievements on Asalith' });
    expect(dialog).toHaveTextContent('3 achievements');
    const regions = within(dialog).getAllByRole('region').map((r) => r.getAttribute('aria-label'));
    expect(regions).toEqual(['List of achievements', 'Minecraft', 'create']);
    const story = within(dialog).getByRole('region', { name: 'Minecraft' });
    expect(story).toHaveTextContent('2 of 16');
    expect(within(story).getByText('Diamonds!').closest('li')).toHaveTextContent('Earned:');
    const create = within(dialog).getByRole('region', { name: 'create' });
    expect(create).toHaveTextContent('Andesite alloy');
    expect(within(create).queryByText(/Not yet/)).toBeNull();
    expect(dialog).toHaveTextContent('Achievements from mods show a name made from their ID.');
  });

  it('leaves the mod note out for vanilla only', () => {
    renderWithProviders(<AchievementsDialog server={server(['minecraft:story/root'])} open onClose={() => {}} />);
    expect(screen.getByRole('dialog')).not.toHaveTextContent(/from mods/);
  });
});
