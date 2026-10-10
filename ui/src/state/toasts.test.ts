import { afterEach, expect, it } from 'vitest';
import { clearToasts, dismissToast, pushError, toastsSnapshot } from './toasts';

afterEach(clearToasts);

it('queues and dismisses errors', () => {
  pushError({ category: 'disk', code: 'disk_io', message: 'x', fix: null, detail: null, params: {} });
  pushError(new Error('boom'));
  const toasts = toastsSnapshot();
  expect(toasts).toHaveLength(2);
  dismissToast(toasts[0]!.id);
  expect(toastsSnapshot()).toHaveLength(1);
});
