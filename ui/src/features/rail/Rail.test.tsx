import { act, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp } from '../../test/backend';
import { Rail } from './Rail';

it('lists instances, then damaged ones, and selects on click', async () => {
  const { user } = renderApp(<Rail />);
  const asalith = await screen.findByRole('button', { name: 'Asalith Fields' });
  expect(screen.getByRole('button', { name: 'Creative Flat' })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'old-pack, Needs attention' })).toBeInTheDocument();
  expect(asalith).toHaveAttribute('aria-current', 'page');
  await user.click(screen.getByRole('button', { name: 'Creative Flat' }));
  expect(routeSnapshot()).toEqual({ name: 'home', id: 'creative-flat' });
});

it('moves focus with the arrow keys and enters at the selected item', async () => {
  const { user } = renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  await user.tab();
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).toHaveFocus();
  await user.keyboard('{ArrowDown}');
  expect(screen.getByRole('button', { name: 'Creative Flat' })).toHaveFocus();
  await user.keyboard('{End}');
  expect(screen.getByRole('button', { name: 'old-pack, Needs attention' })).toHaveFocus();
});

it('marks a running instance', async () => {
  renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(screen.getByRole('button', { name: 'Asalith Fields, Playing' })).toBeInTheDocument();
});
