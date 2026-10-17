import { QueryClientProvider } from '@tanstack/react-query';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, expect, it } from 'vitest';
import { commands } from '../ipc/commands';
import { createMockBackend, type MockOptions } from '../ipc/mock';
import { createQueryClient, qk, useInstanceStats, usePerlandStats, usePerlandStatus, useServerStats, useYourServers } from './queries';
import { clearToasts, toastsSnapshot } from './toasts';

function setup(perland: MockOptions['perland']) {
  const calls: [string, unknown][] = [];
  const backend = createMockBackend({ perland });
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    return backend.handle(cmd, args);
  });
  const qc = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
  return { calls, wrapper, qc };
}

afterEach(() => clearMocks());

it('reads the account status under qk.perland', async () => {
  const { wrapper, qc } = setup('signedIn');
  const { result } = renderHook(() => usePerlandStatus(), { wrapper });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data?.account?.name).toBe('Steve');
  expect(qc.getQueryData(qk.perland)).toEqual(result.current.data);
});

it('asks for Your servers only when enabled', async () => {
  const { wrapper, calls } = setup('signedIn');
  const off = renderHook(() => useYourServers(false), { wrapper });
  await new Promise((r) => setTimeout(r, 20));
  expect(calls.filter(([c]) => c === 'your_servers')).toHaveLength(0);
  off.unmount();
  const { result } = renderHook(() => useYourServers(true), { wrapper });
  await waitFor(() => expect(result.current.data?.servers).toHaveLength(4));
  expect(calls.filter(([c]) => c === 'your_servers')).toHaveLength(1);
});

const statsCalls = (calls: [string, unknown][]) => calls.filter(([c]) => c === 'perland_stats');

it('does not read stats while signed out', async () => {
  const { wrapper, calls } = setup('signedOut');
  const { result } = renderHook(() => usePerlandStats(), { wrapper });
  await waitFor(() => expect(calls.some(([c]) => c === 'perland_status')).toBe(true));
  await new Promise((r) => setTimeout(r, 20));
  expect(statsCalls(calls)).toHaveLength(0);
  expect(result.current.data).toBeUndefined();
});

it('reads the stats again when the linked name changes', async () => {
  const { wrapper, calls, qc } = setup('signedIn');
  const { result } = renderHook(() => usePerlandStats(), { wrapper });
  await waitFor(() => expect(result.current.data).toEqual({ servers: [] }));
  expect(statsCalls(calls)).toHaveLength(1);
  for (let i = 0; i < 3; i++) await commands.perlandLinkPoll();
  await qc.invalidateQueries({ queryKey: qk.perland });
  await waitFor(() => expect(result.current.data?.servers.length).toBeGreaterThan(0));
  expect(statsCalls(calls)).toHaveLength(2);
});

it('finds one server by token and by instance id', async () => {
  const { wrapper } = setup('linked');
  const owner = 'O'.repeat(43);
  const { result } = renderHook(() => ({ byToken: useServerStats(owner), byInstance: useInstanceStats('nope'), shared: useServerStats('F'.repeat(43)) }), { wrapper });
  await waitFor(() => expect(result.current.byToken?.advancements).toBe(12));
  expect(result.current.shared?.playtimeSeconds).toBe(2400);
  expect(result.current.byInstance).toBeUndefined();
});

it('keeps quiet when stats fail', async () => {
  clearToasts();
  const qc = createQueryClient();
  mockIPC((cmd) => {
    if (cmd === 'perland_status') return { enabled: true, methods: [], account: { name: 'Steve', email: null, discord: true, newPlayer: false, minecraft: null } };
    throw { category: 'network', code: 'engine_unreachable', message: 'x', fix: null, detail: null, params: {} };
  });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
  const { result } = renderHook(() => usePerlandStats(), { wrapper });
  await waitFor(() => expect(result.current.isError).toBe(true));
  expect(toastsSnapshot()).toHaveLength(0);
});
