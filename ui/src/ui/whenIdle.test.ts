import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { whenIdle } from './whenIdle';

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it('waits the delay, then runs once the window is idle', () => {
  const idle = vi.fn((run: () => void, options?: { timeout: number }) => {
    void options;
    setTimeout(run, 50);
    return 7;
  });
  vi.stubGlobal('requestIdleCallback', idle);
  vi.stubGlobal('cancelIdleCallback', vi.fn());
  const work = vi.fn();
  whenIdle(work, 3000);
  vi.advanceTimersByTime(2999);
  expect(idle).not.toHaveBeenCalled();
  vi.advanceTimersByTime(1);
  // A busy window still gets to it, a little later.
  expect(idle).toHaveBeenCalledWith(expect.any(Function), { timeout: expect.any(Number) });
  expect(work).not.toHaveBeenCalled();
  vi.advanceTimersByTime(50);
  expect(work).toHaveBeenCalledTimes(1);
});

it('runs after the delay where the webview has no idle callback', () => {
  vi.stubGlobal('requestIdleCallback', undefined);
  const work = vi.fn();
  whenIdle(work, 3000);
  vi.advanceTimersByTime(3000);
  expect(work).not.toHaveBeenCalled();
  vi.advanceTimersByTime(1000);
  expect(work).toHaveBeenCalledTimes(1);
});

it('does nothing once cancelled', () => {
  vi.stubGlobal('requestIdleCallback', undefined);
  const work = vi.fn();
  const cancel = whenIdle(work, 3000);
  vi.advanceTimersByTime(1000);
  cancel();
  vi.advanceTimersByTime(10_000);
  expect(work).not.toHaveBeenCalled();
});
