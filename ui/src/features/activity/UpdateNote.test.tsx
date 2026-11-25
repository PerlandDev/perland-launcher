import { act, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { beginContentInstall } from '../../state/contentProgress';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { ActivityDrawer } from './ActivityDrawer';

const ready = { beta: true, offlineLocked: true, update: { state: 'ready', version: '0.2.0-beta.2' } } as const;

it('says in the Activity bar, not the title bar, that an update is ready, with Restart now', async () => {
  const { user, calls } = renderApp(<ActivityDrawer />, ready);
  expect(await screen.findByText('Update ready · installs when you close')).toBeVisible();
  // The bar's own toggle keeps its name.
  expect(screen.getByRole('button', { name: 'Activity' })).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Restart now' }));
  expect(sent(calls, 'update_restart')).toHaveLength(1);
});

it('sends one restart for a double click', async () => {
  const { user, backend, calls } = renderApp(<ActivityDrawer />, ready);
  // A real restart never answers: the launcher closes. Park it, so the second click lands while it's in flight.
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'update_restart' ? new Promise(() => {}) : handle(cmd, args)));
  await user.dblClick(await screen.findByRole('button', { name: 'Restart now' }));
  expect(sent(calls, 'update_restart')).toHaveLength(1);
});

it('disables Restart now while a game runs and says why', async () => {
  renderApp(<ActivityDrawer />, ready);
  const restart = await screen.findByRole('button', { name: 'Restart now' });
  expect(restart).toBeEnabled();
  expect(restart).not.toHaveAccessibleDescription();
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeDisabled();
  // Visible text, not a hover title: a disabled button can't be focused to show one.
  expect(screen.getByText('Update ready · wait for games and installs to finish')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Restart now' })).toHaveAccessibleDescription('Update ready · wait for games and installs to finish');
});

it('disables Restart now while a Browse install runs', async () => {
  renderApp(<ActivityDrawer />, ready);
  expect(await screen.findByRole('button', { name: 'Restart now' })).toBeEnabled();
  let end = () => {};
  act(() => {
    end = beginContentInstall();
  });
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeDisabled();
  act(() => end());
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeEnabled();
});

it('says nothing when no update is ready', async () => {
  renderApp(<ActivityDrawer />, { beta: true, offlineLocked: true, update: { state: 'up_to_date' } });
  await screen.findByRole('button', { name: 'Activity' });
  expect(screen.queryByText(/Update ready/)).toBeNull();
  expect(screen.queryByRole('button', { name: 'Restart now' })).toBeNull();
});
