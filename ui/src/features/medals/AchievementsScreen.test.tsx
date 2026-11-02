import { screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { renderApp } from '../../test/backend';
import { AchievementsScreen } from './AchievementsScreen';

it('shows the medals and the Perland servers, and goes back to Home', async () => {
  const { user } = renderApp(<AchievementsScreen />, { perland: 'linked' });
  expect(await screen.findByRole('heading', { name: 'Achievements', level: 1 })).toBeInTheDocument();
  expect(screen.getByRole('heading', { name: 'Medals' })).toBeInTheDocument();
  expect(await screen.findByRole('heading', { name: 'Achievements on Perland servers' })).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Back' }));
  expect(routeSnapshot()).toEqual({ name: 'home', id: null });
});
