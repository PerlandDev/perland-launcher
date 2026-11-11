import { act, screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import type { UpdateState } from '../../ipc/schema';
import { beginContentInstall } from '../../state/contentProgress';
import { qk } from '../../state/queries';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { Settings } from './Settings';

vi.mock('../whats-new/notes', () => ({
  releaseNotesData: {
    versions: ['2026.11.5'],
    groups: [
      { base: null, date: '2026-11-21', notes: [{ id: 'e', kind: 'new', since: null, en: 'Coming soon.', es: 'Pronto.' }] },
      { base: '2026.11.5', date: '2026-11-12', notes: [{ id: 'a', kind: 'improved', since: '2026.11.5', en: 'Skins save faster.', es: 'Las skins se guardan más rápido.' }] },
    ],
  },
}));

it('shows the state, checks now and switches channel', async () => {
  const { user, calls } = renderApp(<Settings />, { beta: true, offlineLocked: true, update: { state: 'up_to_date' } });
  expect(await screen.findByText(/Up to date · 0\.1\.0/)).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Check now' }));
  expect(sent(calls, 'update_check')).toHaveLength(1);
  // Two channels: the standard pressed segments, so the choice lights the lime lamp.
  expect(screen.getByRole('group', { name: 'Update channel' })).toHaveAccessibleDescription('New builds first, with a few rough edges.');
  expect(screen.getByRole('radio', { name: 'Beta' })).toBeChecked();
  await user.click(screen.getByRole('radio', { name: 'Stable' }));
  expect(await screen.findByText("You'll move to stable once it's newer than your beta.")).toBeInTheDocument();
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { updateChannel: 'stable' } });
});

it('offers Restart now when an update is ready', async () => {
  const { user, calls } = renderApp(<Settings />, { beta: true, offlineLocked: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
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
  const { qc } = renderApp(<Settings />, { beta: true, offlineLocked: true, update: { state: 'downloading', percent: 40 } });
  // The live region says the state once; the percent lives outside it, on a blue meter.
  expect(await screen.findByText('Downloading the update…', { selector: '[role=status]' })).toHaveTextContent(/^Downloading the update…$/);
  expect(screen.getByRole('progressbar', { name: 'Downloading the update…' })).toHaveAttribute('aria-valuenow', '40');
  expect(screen.getByText('40%')).toBeInTheDocument();
  // handleEvent writes an `update` event here (covered in handleEvent.test.ts).
  const failed: UpdateState = {
    state: 'failed',
    error: { category: 'network', code: 'launcher_update_failed', message: 'x', fix: null, detail: null, params: {} },
  };
  act(() => qc.setQueryData(qk.update, failed));
  // The error's own words and its way forward, so a bad download isn't called a failed check.
  expect(await screen.findByText("Couldn't check for launcher updates.")).toBeInTheDocument();
  expect(screen.getByText('You can keep playing. It tries again next time you open the launcher.')).toBeInTheDocument();
  expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
});

it('shows the state Check now returns even if the update event is missed', async () => {
  const { user } = renderApp(<Settings />, { beta: true, offlineLocked: true, update: { state: 'idle' } });
  await user.click(await screen.findByRole('button', { name: 'Check now' }));
  expect(await screen.findByText(/Up to date · 0\.1\.0/)).toBeInTheDocument();
});

it('disables Restart now and explains why while a game runs', async () => {
  renderApp(<Settings />, { beta: true, offlineLocked: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  const restart = await screen.findByRole('button', { name: 'Restart now' });
  expect(restart).toBeEnabled();
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Restart now' })).toHaveAccessibleDescription('Wait for games and installs to finish first.');
});

it('disables Restart now while a Browse install runs', async () => {
  renderApp(<Settings />, { beta: true, offlineLocked: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  expect(await screen.findByRole('button', { name: 'Restart now' })).toBeEnabled();
  let end = () => {};
  act(() => {
    end = beginContentInstall();
  });
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Restart now' })).toHaveAccessibleDescription('Wait for games and installs to finish first.');
  act(() => end());
  expect(screen.getByRole('button', { name: 'Restart now' })).toBeEnabled();
});

it.each([
  ['a release build', { release: true }],
  ['a dev build', {}],
])("opens What's new with every version in %s", async (_name, options) => {
  const { user, calls } = renderApp(<Settings />, options);
  await user.click(await screen.findByRole('button', { name: "What's new" }));
  const dialog = screen.getByRole('dialog', { name: "What's new" });
  expect(within(dialog).getAllByRole('heading', { level: 3 }).map((h) => h.textContent)).toEqual(['Next update', '2026.11.5 · Nov 12']);
  expect(within(dialog).queryByRole('button', { name: 'All updates' })).not.toBeInTheDocument();
  // Reading the history is not the launch check: the marker stays where it was.
  expect(sent(calls, 'whats_new_mark')).toHaveLength(0);
  await user.click(within(dialog).getByRole('button', { name: 'Got it' }));
  expect(screen.queryByRole('dialog', { name: "What's new" })).not.toBeInTheDocument();
});
