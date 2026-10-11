import { renderHook } from '@testing-library/react';
import { expect, it } from 'vitest';
import { exitOf } from '../ipc/mock';
import { resetActivity, useActivity } from './activity';
import { handleEvent } from './handleEvent';
import { runStore } from './runStore';
import { clearToasts, toastsSnapshot } from './toasts';

it('turns warnings into toasts and opens the drawer on a crash', () => {
  clearToasts();
  resetActivity();
  handleEvent({
    type: 'warning',
    instance: 'pack',
    error: { category: 'disk', code: 'log_write_stopped', message: 'x', fix: null, detail: null, params: {} },
  });
  expect(toastsSnapshot()).toHaveLength(1);
  handleEvent({
    type: 'run_state',
    instance: 'pack',
    state: 'idle',
    exit: exitOf({ code: 1 }),
    update: null,
  });
  const { result } = renderHook(() => useActivity());
  expect(result.current).toEqual({ open: true, focus: 'pack' });
  expect(runStore.run('pack').lastExit?.code).toBe(1);
});

it('leaves the drawer alone for a requested stop', () => {
  resetActivity();
  handleEvent({
    type: 'run_state',
    instance: 'pack',
    state: 'idle',
    exit: exitOf({ killed: true }),
    update: null,
  });
  const { result } = renderHook(() => useActivity());
  expect(result.current.open).toBe(false);
});

it('opens the drawer when a game fails to start', () => {
  resetActivity();
  const error = { category: 'game', code: 'java_start_failed', message: 'x', fix: null, detail: null, params: {} } as const;
  handleEvent({
    type: 'run_state',
    instance: 'pack',
    state: 'idle',
    exit: exitOf({ error }),
    update: null,
  });
  const { result } = renderHook(() => useActivity());
  expect(result.current).toEqual({ open: true, focus: 'pack' });
});
