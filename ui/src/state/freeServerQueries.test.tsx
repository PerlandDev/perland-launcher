import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { createMockBackend, stopMockRuns } from '../ipc/mock';
import type { MockOptions } from '../ipc/mock';
import { createQueryClient, freeServerInterval, qk, useFreeServer, useFreeServerCreate, useFreeServerDelete, useFreeServerPlay } from './queries';

afterEach(() => {
  clearMocks();
  stopMockRuns();
  vi.useRealTimers();
});

function setup(free: MockOptions['free'] = 'none') {
  const calls: [string, unknown][] = [];
  const backend = createMockBackend({ perland: 'signedIn', free });
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    return backend.handle(cmd, args);
  });
  const qc: QueryClient = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
  return { calls, wrapper, qc };
}

it('polls every 30 s, and every 15 s while creating, starting or deleting', () => {
  const page = (freeStatus?: string) => (freeStatus ? { server: { freeStatus } } : { server: null });
  expect(freeServerInterval(undefined)).toBe(30_000);
  expect(freeServerInterval(null)).toBe(30_000);
  expect(freeServerInterval(page() as never)).toBe(30_000);
  for (const status of ['online', 'sleeping', 'offline', 'stopped']) expect(freeServerInterval(page(status) as never)).toBe(30_000);
  for (const status of ['creating', 'starting', 'deleting']) expect(freeServerInterval(page(status) as never)).toBe(15_000);
});

it('reads only when enabled', async () => {
  const off = setup('has');
  const { result } = renderHook(() => useFreeServer(false), { wrapper: off.wrapper });
  expect(result.current.fetchStatus).toBe('idle');
  expect(off.calls.filter(([c]) => c === 'free_server')).toEqual([]);
  const on = setup('has');
  const hook = renderHook(() => useFreeServer(true), { wrapper: on.wrapper });
  await waitFor(() => expect(hook.result.current.data?.server?.name).toBe('Weekend SMP'));
  expect(on.qc.getQueryCache().find({ queryKey: qk.freeServer })).toBeDefined();
});

it('refetches after 15 s while the server is being created', async () => {
  const { wrapper, calls } = setup('creating');
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const { result } = renderHook(() => useFreeServer(true), { wrapper });
  await waitFor(() => expect(result.current.data?.server?.freeStatus).toBe('creating'));
  expect(calls.filter(([c]) => c === 'free_server')).toHaveLength(1);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(15_100);
  });
  expect(calls.filter(([c]) => c === 'free_server')).toHaveLength(2);
});

it('create and delete invalidate the free server and Your servers', async () => {
  const { wrapper, qc } = setup('none');
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const spy = vi.spyOn(qc, 'invalidateQueries');
  const create = renderHook(() => useFreeServerCreate({ inline: true }), { wrapper });
  const done = act(async () => {
    const pending = create.result.current.mutateAsync({ name: 'Weekend', kind: 'vanilla', modrinthVersionId: null, language: 'en' });
    await vi.advanceTimersByTimeAsync(2100);
    await pending;
  });
  await done;
  const keys = () => spy.mock.calls.map(([f]) => (f as { queryKey: unknown }).queryKey);
  expect(keys()).toEqual(expect.arrayContaining([qk.freeServer, qk.yourServers]));
  spy.mockClear();
  const del = renderHook(() => useFreeServerDelete({ inline: true }), { wrapper });
  await act(async () => {
    await del.result.current.mutateAsync();
  });
  expect(keys()).toEqual(expect.arrayContaining([qk.freeServer, qk.yourServers]));
});

it('play answers the instance id and invalidates both', async () => {
  const { wrapper, qc } = setup('has');
  const spy = vi.spyOn(qc, 'invalidateQueries');
  const play = renderHook(() => useFreeServerPlay({ inline: true }), { wrapper });
  let id = '';
  await act(async () => {
    id = await play.result.current.mutateAsync();
  });
  expect(id).toMatch(/^[a-z0-9-]+$/);
  expect(spy.mock.calls.map(([f]) => (f as { queryKey: unknown }).queryKey)).toEqual(expect.arrayContaining([qk.freeServer, qk.yourServers]));
});
