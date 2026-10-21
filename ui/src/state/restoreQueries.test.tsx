import { QueryClientProvider } from '@tanstack/react-query';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, expect, it } from 'vitest';
import { createQueryClient, qk, useContentRemove, useModMutations, usePackStatus } from './queries';

function setup(handle: (cmd: string) => unknown) {
  mockIPC((cmd) => handle(cmd));
  const qc = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
  return { qc, wrapper };
}

const invalidated = (qc: ReturnType<typeof createQueryClient>, queryKey: readonly unknown[]) =>
  qc.getQueryCache().find({ queryKey })?.state.isInvalidated;

afterEach(() => clearMocks());

it('asks for the restore point again after a mod is removed', async () => {
  const { qc, wrapper } = setup(() => []);
  qc.setQueryData(qk.restorePoint('sky'), null);
  const { result } = renderHook(() => useModMutations('sky'), { wrapper });
  await act(() => result.current.remove.mutateAsync('a.jar'));
  expect(invalidated(qc, qk.restorePoint('sky'))).toBe(true);
});

it('asks for the restore point again after a resource pack is removed', async () => {
  const { qc, wrapper } = setup(() => []);
  qc.setQueryData(qk.restorePoint('sky'), null);
  const { result } = renderHook(() => useContentRemove('sky', 'resource_pack'), { wrapper });
  await act(() => result.current.mutateAsync('Faithful.zip'));
  expect(invalidated(qc, qk.restorePoint('sky'))).toBe(true);
});

it('asks for the instances again when the pack status lifted the hold', async () => {
  let lifted = true;
  const { qc, wrapper } = setup(() => ({ installedVersion: '1.0.3', update: null, checkError: null, holdLifted: lifted }));
  qc.setQueryData(qk.instances, []);
  const { result } = renderHook(() => usePackStatus('sky', true), { wrapper });
  await waitFor(() => expect(result.current.data).toBeDefined());
  expect(invalidated(qc, qk.instances)).toBe(true);

  lifted = false;
  qc.setQueryData(qk.instances, []);
  await act(() => result.current.refetch());
  expect(invalidated(qc, qk.instances)).toBe(false);
});
