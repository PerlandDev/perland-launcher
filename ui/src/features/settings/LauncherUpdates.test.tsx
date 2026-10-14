import { act, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import type { UpdateState } from '../../ipc/schema';
import { qk } from '../../state/queries';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { Settings } from './Settings';

it('shows the state, checks now and switches channel', async () => {
  const { user, calls } = renderApp(<Settings />, { beta: true, update: { state: 'up_to_date' } });
  expect(await screen.findByText(/Up to date · 0\.1\.0/)).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Check now' }));
  expect(sent(calls, 'update_check')).toHaveLength(1);
  const channel = screen.getByRole('combobox', { name: 'Update channel' });
  expect(channel).toHaveValue('beta');
  await user.selectOptions(channel, 'stable');
  expect(await screen.findByText("You'll move to stable once it's newer than your beta.")).toBeInTheDocument();
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { updateChannel: 'stable' } });
});

it('offers Restart now when an update is ready', async () => {
  const { user, calls } = renderApp(<Settings />, { beta: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  expect(await screen.findByText('Version 0.2.0-beta.2 is ready. It installs when you close the launcher.')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Restart now' }));
  expect(sent(calls, 'update_restart')).toHaveLength(1);
});

it('says updates are off in dev builds', async () => {
  renderApp(<Settings />);
  expect(await screen.findByText('Updates are off in developer builds.')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Check now' })).not.toBeInTheDocument();
});

it('shows download progress and a failed check', async () => {
  const { qc } = renderApp(<Settings />, { beta: true, update: { state: 'downloading', percent: 40 } });
  expect(await screen.findByText('Downloading the update… 40%')).toBeInTheDocument();
  // handleEvent writes an `update` event here (covered in handleEvent.test.ts).
  const failed: UpdateState = {
    state: 'failed',
    error: { category: 'network', code: 'launcher_update_failed', message: 'x', fix: null, detail: null, params: {} },
  };
  act(() => qc.setQueryData(qk.update, failed));
  expect(await screen.findByText("Couldn't check for updates.")).toBeInTheDocument();
});

it('shows the state Check now returns even if the update event is missed', async () => {
  const { user } = renderApp(<Settings />, { beta: true, update: { state: 'idle' } });
  await user.click(await screen.findByRole('button', { name: 'Check now' }));
  expect(await screen.findByText(/Up to date · 0\.1\.0/)).toBeInTheDocument();
});

it('disables Restart now and explains why while a game runs', async () => {
  renderApp(<Settings />, { beta: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  const restart = await screen.findByRole('button', { name: 'Restart now' });
  expect(restart).toBeEnabled();
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeDisabled();
  expect(screen.getByText('Close your games first.')).toBeInTheDocument();
});
