import { afterEach, expect, it, vi } from 'vitest';
import { installErrorReporting, REPORT_CAP } from './reportErrors';

// A target of its own: an error event on the real window would count as the test's own uncaught error.
let target = new EventTarget() as unknown as Window;
let stop = () => {};
afterEach(() => {
  stop();
  target = new EventTarget() as unknown as Window;
});

function setup(send = vi.fn(() => Promise.resolve(null))) {
  stop = installErrorReporting(target, send);
  return send;
}

const throwError = (error: unknown, message = '') => target.dispatchEvent(new ErrorEvent('error', { error, message }));
const reject = (reason: unknown) => {
  const event = new Event('unhandledrejection') as Event & { reason?: unknown };
  event.reason = reason;
  target.dispatchEvent(event);
};

it('reports an uncaught error and an unhandled rejection with their kind', () => {
  const send = setup();
  const boom = new TypeError('boom');
  throwError(boom);
  reject(new Error('later'));
  expect(send).toHaveBeenCalledTimes(2);
  expect(send).toHaveBeenNthCalledWith(1, 'exception', 'TypeError: boom', boom.stack);
  expect(send).toHaveBeenNthCalledWith(2, 'unhandled_rejection', 'Error: later', expect.any(String));
});

it('reports the same error once and stops at the cap', () => {
  const send = setup();
  const boom = new Error('boom');
  for (let n = 0; n < 50; n += 1) throwError(boom);
  expect(send).toHaveBeenCalledTimes(1);
  for (let n = 0; n < 50; n += 1) throwError(new Error(`different ${n}`));
  expect(send).toHaveBeenCalledTimes(REPORT_CAP);
});

it('never sends an object that was thrown, and leaves command errors to Rust', () => {
  const send = setup();
  reject({ category: 'network', code: 'offline', message: 'You seem to be offline', params: { host: 'play.example.com' } });
  expect(send).not.toHaveBeenCalled();
  reject({ token: 'secret', name: 'Steve' });
  expect(send).toHaveBeenCalledWith('unhandled_rejection', 'Non-error thrown (object)', null);
  throwError(undefined, 'Script error.');
  expect(send).toHaveBeenLastCalledWith('exception', 'Script error.', null);
});

it('cuts long text and survives a send that fails or throws', async () => {
  const send = setup(vi.fn(() => Promise.reject(new Error('no backend'))));
  const long = new Error('x'.repeat(9000));
  long.stack = 'y'.repeat(50000);
  expect(() => throwError(long)).not.toThrow();
  const [, message, stack] = send.mock.calls[0] as unknown as [string, string, string];
  expect(message).toHaveLength(4000);
  expect(stack).toHaveLength(24000);
  await Promise.resolve();
  stop();
  stop = installErrorReporting(target, () => {
    throw new Error('sync failure');
  });
  expect(() => throwError(new Error('again'))).not.toThrow();
});

it('stops listening when asked', () => {
  const send = setup();
  stop();
  throwError(new Error('after'));
  expect(send).not.toHaveBeenCalled();
});
