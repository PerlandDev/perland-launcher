import { QueryClientProvider } from '@tanstack/react-query';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, expect, it } from 'vitest';
import { createMockBackend } from '../ipc/mock';
import type { ListQuery } from '../ipc/schema';
import { createQueryClient, useServerIcon, useServersList } from './queries';

const query: ListQuery = { sort: 'players', modded: null, minecraft: null, hasRoom: false, category: null, language: null, region: null, offset: 0 };

function setup(total = 8) {
  const calls: [string, unknown][] = [];
  const backend = createMockBackend();
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'servers_list' && total > 8) {
      const page = backend.handle(cmd, args) as Promise<{ servers: unknown[]; total: number }>;
      return page.then((p) => ({ ...p, total }));
    }
    return backend.handle(cmd, args);
  });
  const qc = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
  return { calls, wrapper, qc };
}

afterEach(() => clearMocks());

it('keys the list by its query and has no next page when everything fit', async () => {
  const { wrapper, qc } = setup();
  const { result } = renderHook(() => useServersList(query), { wrapper });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(result.current.data?.pages[0]?.servers).toHaveLength(8);
  expect(result.current.hasNextPage).toBe(false);
  expect(qc.getQueryCache().find({ queryKey: ['servers', query] })).toBeDefined();
});

it('asks for the next page at offset + 50 while that is below the total', async () => {
  const { wrapper, calls } = setup(120);
  const { result } = renderHook(() => useServersList(query), { wrapper });
  await waitFor(() => expect(result.current.hasNextPage).toBe(true));
  await result.current.fetchNextPage();
  await waitFor(() => expect(calls.filter(([c]) => c === 'servers_list')).toHaveLength(2));
  expect(calls.filter(([c]) => c === 'servers_list').map(([, a]) => (a as { query: ListQuery }).query.offset)).toEqual([0, 50]);
  await waitFor(() => expect(result.current.hasNextPage).toBe(true));
  await result.current.fetchNextPage();
  await waitFor(() => expect(result.current.hasNextPage).toBe(false));
});

it('asks for an icon only when there is an icon version', async () => {
  const { wrapper, calls } = setup();
  const none = renderHook(() => useServerIcon('A'.repeat(43), null), { wrapper });
  expect(none.result.current.fetchStatus).toBe('idle');
  const some = renderHook(() => useServerIcon('A'.repeat(43), '1a2b3c4d5e6f7a8b'), { wrapper });
  await waitFor(() => expect(some.result.current.data).toMatch(/^data:image\//));
  expect(calls.filter(([c]) => c === 'servers_icon')).toEqual([['servers_icon', { token: 'A'.repeat(43), iconVersion: '1a2b3c4d5e6f7a8b' }]]);
});
