import { QueryClientProvider } from '@tanstack/react-query';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, expect, it } from 'vitest';
import { createMockBackend, type MockOptions } from '../ipc/mock';
import { createQueryClient, qk, usePerlandStatus, useYourServers } from './queries';

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
