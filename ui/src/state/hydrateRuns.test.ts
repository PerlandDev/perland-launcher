import { mockIPC } from '@tauri-apps/api/mocks';
import { expect, it } from 'vitest';
import { hydrateRuns } from './hydrateRuns';
import { createRunStore } from './runStore';
import { toastsSnapshot } from './toasts';

const failure = { category: 'game', code: 'internal', message: 'boom', fix: null, detail: null, params: {} };

it('restores running games and their recent output', async () => {
  mockIPC((cmd, args) => {
    if (cmd === 'run_states') return [{ instance: 'pack', state: 'running', update: null }];
    if (cmd === 'log_tail') return [`line from ${(args as { id: string }).id}`];
    return null;
  });
  const store = createRunStore();
  await hydrateRuns(store);
  expect(store.run('pack').state).toBe('running');
  expect(store.log('pack')).toEqual(['line from pack']);
  expect(toastsSnapshot()).toHaveLength(0);
});

it('carries on with an empty list when the run states cannot be read', async () => {
  mockIPC((cmd) => (cmd === 'run_states' ? Promise.reject(failure) : null));
  const store = createRunStore();
  await expect(hydrateRuns(store)).resolves.toBeUndefined();
  expect(store.active()).toEqual([]);
  expect(toastsSnapshot()).toHaveLength(1);
});

it('keeps the games it could read when some log tails fail, and reports once', async () => {
  mockIPC((cmd, args) => {
    if (cmd === 'run_states') {
      return [
        { instance: 'a', state: 'running', update: null },
        { instance: 'b', state: 'running', update: null },
        { instance: 'c', state: 'preparing', update: null },
      ];
    }
    if (cmd === 'log_tail') {
      const id = (args as { id: string }).id;
      return id === 'a' || id === 'c' ? Promise.reject(failure) : ['b output'];
    }
    return null;
  });
  const store = createRunStore();
  await hydrateRuns(store);
  expect([...store.active()].sort()).toEqual(['a', 'b', 'c']);
  expect(store.log('b')).toEqual(['b output']);
  expect(store.log('a')).toEqual([]);
  expect(toastsSnapshot()).toHaveLength(1);
});

it('does not undo a live event that arrives while the log tails are being fetched', async () => {
  const store = createRunStore();
  mockIPC((cmd) => {
    if (cmd === 'run_states') return [{ instance: 'pack', state: 'running', update: null }];
    if (cmd === 'log_tail') {
      // The game exits and prints a last line while the tail is still on its way.
      store.apply({ type: 'log', instance: 'pack', lines: ['bye'] });
      store.apply({ type: 'run_state', instance: 'pack', state: 'idle', exit: null, update: null });
      return ['old'];
    }
    return null;
  });
  await hydrateRuns(store);
  expect(store.run('pack').state).toBe('idle');
  expect(store.log('pack')).toEqual(['bye']);
});
