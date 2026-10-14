import { act, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { TitleBar } from './TitleBar';

it('drives the window and shows the active account', async () => {
  const { user, calls } = renderApp(<TitleBar />);
  expect(await screen.findByRole('button', { name: /Steve/ })).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Minimize' }));
  await user.click(screen.getByRole('button', { name: 'Maximize' }));
  expect(sent(calls, 'plugin:window|minimize')).toHaveLength(1);
  expect(sent(calls, 'plugin:window|toggle_maximize')).toHaveLength(1);
  await user.click(screen.getByRole('button', { name: 'Settings' }));
  expect(routeSnapshot()).toEqual({ name: 'settings' });
  await user.click(screen.getByRole('button', { name: /Steve/ }));
  expect(routeSnapshot()).toEqual({ name: 'accounts' });
});

it('keeps only the window controls when minimal', async () => {
  renderApp(<TitleBar minimal />, { firstRun: true });
  expect(await screen.findByRole('button', { name: 'Close' })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Minimize' })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Add account' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Settings' })).not.toBeInTheDocument();
});

it('asks for an account when there is none', async () => {
  renderApp(<TitleBar />, { firstRun: true });
  expect(await screen.findByRole('button', { name: 'Add account' })).toBeInTheDocument();
});

it('shows a quiet note with Restart now when an update is ready', async () => {
  const { user, calls } = renderApp(<TitleBar />, { beta: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  expect(await screen.findByText('Update ready · installs when you close')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Restart now' }));
  expect(sent(calls, 'update_restart')).toHaveLength(1);
});

it('disables Restart now while a game runs', async () => {
  renderApp(<TitleBar />, { beta: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  expect(await screen.findByRole('button', { name: 'Restart now' })).toBeEnabled();
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeDisabled();
});

it('shows no note when nothing is ready, and none in the minimal bar', async () => {
  const { unmount } = renderApp(<TitleBar />, { beta: true, update: { state: 'up_to_date' } });
  await screen.findByRole('button', { name: 'Close' });
  expect(screen.queryByText('Update ready · installs when you close')).not.toBeInTheDocument();
  unmount();
  renderApp(<TitleBar minimal />, { beta: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  await screen.findByRole('button', { name: 'Close' });
  expect(screen.queryByText('Update ready · installs when you close')).not.toBeInTheDocument();
});
