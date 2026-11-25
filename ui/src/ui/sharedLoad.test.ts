import { expect, it, vi } from 'vitest';
import { sharedLoad } from './sharedLoad';

it('loads once, so a prefetch and the screen share the same download', async () => {
  const load = vi.fn(() => Promise.resolve('chunk'));
  const get = sharedLoad(load);
  const first = get();
  expect(get()).toBe(first);
  await expect(first).resolves.toBe('chunk');
  expect(get()).toBe(first);
  expect(load).toHaveBeenCalledTimes(1);
});

it('loads again after a failure, so Try again really tries', async () => {
  const load = vi.fn<() => Promise<string>>().mockRejectedValueOnce(new Error('stale file')).mockResolvedValueOnce('chunk');
  const get = sharedLoad(load);
  await expect(get()).rejects.toThrow('stale file');
  await expect(get()).resolves.toBe('chunk');
  expect(load).toHaveBeenCalledTimes(2);
});
