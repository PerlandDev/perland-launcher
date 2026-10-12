import { afterEach, expect, it, vi } from 'vitest';
import { clearToasts, dismissToast, pushError, pushNotice, toastsSnapshot } from './toasts';

afterEach(clearToasts);

it('queues and dismisses errors', () => {
  pushError({ category: 'disk', code: 'disk_io', message: 'x', fix: null, detail: null, params: {} });
  pushError(new Error('boom'));
  const toasts = toastsSnapshot();
  expect(toasts).toHaveLength(2);
  dismissToast(toasts[0]!.id);
  expect(toastsSnapshot()).toHaveLength(1);
});

it('keeps notices next to errors', () => {
  const run = vi.fn();
  pushNotice({ title: 'Added Sodium to Asalith', body: 'Restart the game to use it.', action: { label: 'Open', run } });
  pushError({ category: 'pack', code: 'content_busy', message: 'm', fix: null, detail: null, params: {} });
  const [notice, error] = toastsSnapshot();
  expect(notice?.notice?.title).toBe('Added Sodium to Asalith');
  expect(error?.error).toBeDefined();
  notice?.notice?.action?.run();
  expect(run).toHaveBeenCalled();
});
