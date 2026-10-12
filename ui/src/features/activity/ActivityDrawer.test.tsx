import { act, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { exitOf } from '../../ipc/mock';
import { openActivity } from '../../state/activity';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { ActivityDrawer } from './ActivityDrawer';
import { LogView } from './LogView';

describe('ActivityDrawer', () => {
  // jsdom has no layout, so the virtualized log would measure a 0 x 0 viewport and render no rows.
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(320);
  });

  it('opens from its handle and shows the game log', async () => {
    const { user } = renderApp(<ActivityDrawer />);
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null });
      runStore.apply({ type: 'log', instance: 'asalith-fields', lines: ['[INFO] Hello', '[INFO] World'] });
    });
    await user.click(screen.getByRole('button', { name: /Activity/ }));
    const log = await screen.findByRole('log', { name: 'Game log' });
    expect(log).toHaveTextContent('[INFO] Hello');
    expect(log).toHaveTextContent('[INFO] World');
  });

  it('explains an out-of-memory crash and raises memory in one click', async () => {
    const { user, calls } = renderApp(<ActivityDrawer />);
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'queued', exit: null, update: null });
      runStore.apply({
        type: 'run_state',
        instance: 'asalith-fields',
        state: 'idle',
        exit: exitOf({ code: 1, crash: 'out_of_memory' }),
        update: null,
      });
      openActivity('asalith-fields');
    });
    expect(await screen.findByText('Minecraft ran out of memory')).toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: 'Raise to 6 GB' }));
    const update = sent(calls, 'instance_update')[0] as { instance: { java: { memoryMb: number } } };
    expect(update.instance.java.memoryMb).toBe(6144);
  });

  it('shows the pack sync progress, but no meter while only checking', async () => {
    renderApp(<ActivityDrawer />);
    const task = { key: 'install_pack', params: { name: 'Sky Islands' } };
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'checking', exit: null, update: null });
      runStore.apply({ type: 'progress', instance: 'sky-islands', task, done: 0, total: 1, bytes: 0 });
      openActivity('sky-islands');
    });
    await screen.findByText("The game hasn't printed anything yet.");
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'syncing', exit: null, update: null });
      runStore.apply({ type: 'progress', instance: 'sky-islands', task, done: 2, total: 24, bytes: 0 });
    });
    expect(screen.getByRole('progressbar', { name: 'Installing Sky Islands' })).toBeInTheDocument();
    expect(screen.getByText('2 of 24 files', { exact: false })).toBeInTheDocument();
  });

  it('says "file" for a single file', async () => {
    renderApp(<ActivityDrawer />);
    const task = { key: 'install_pack', params: { name: 'Sky Islands' } };
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'syncing', exit: null, update: null });
      runStore.apply({ type: 'progress', instance: 'sky-islands', task, done: 0, total: 1, bytes: 0 });
      openActivity('sky-islands');
    });
    expect(await screen.findByText('0 of 1 file', { exact: false })).toBeInTheDocument();
    expect(screen.queryByText('0 of 1 files', { exact: false })).not.toBeInTheDocument();
  });

  it('shows a failed start with the translated error', async () => {
    renderApp(<ActivityDrawer />);
    act(() => {
      runStore.apply({
        type: 'run_state',
        instance: 'creative-flat',
        state: 'idle',
        exit: exitOf({
          error: { category: 'network', code: 'offline', message: 'x', fix: null, detail: null, params: {} },
        }),
        update: null,
      });
      openActivity('creative-flat');
    });
    expect(await screen.findByText("Creative Flat couldn't start")).toBeInTheDocument();
    expect(screen.getByText("You seem to be offline, and this hasn't been downloaded before.")).toBeInTheDocument();
  });
});

describe('LogView', () => {
  it('virtualizes: 200 lines in a 320px viewport render far fewer rows', () => {
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(320);
    const lines = Array.from({ length: 200 }, (_, i) => `line ${i}`);
    renderApp(<LogView lines={lines} />);
    const log = screen.getByRole('log', { name: 'Game log' });
    const rows = log.firstElementChild?.children.length ?? 0;
    expect(rows).toBeGreaterThan(0);
    expect(rows).toBeLessThan(100);
    expect(log).toHaveAttribute('tabindex', '0');
  });
});
