import { QueryClientProvider } from '@tanstack/react-query';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, expect, it } from 'vitest';
import { createMockBackend } from '../ipc/mock';
import type { InstanceListing } from '../ipc/schema';
import { createQueryClient, qk, useDeleteInstance, usePreviewBannerView, useRenameInstance, useSetBannerView } from './queries';

function setup() {
  const backend = createMockBackend();
  mockIPC((cmd, args) => backend.handle(cmd, args));
  const qc = createQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
  return { qc, wrapper };
}

const PICTURE = { icon: 'data:image/webp;base64,AAAA', banner: null, packBanner: false };

/** Every cached query of instance `id` that outlives its folder: its pictures, files and statuses. */
function seed(qc: ReturnType<typeof createQueryClient>, id: string) {
  qc.setQueryData(qk.instanceArt(id), PICTURE);
  qc.setQueryData(qk.instanceScreenshots(id), ['a.png']);
  qc.setQueryData(qk.instanceScreenshot(id, 'a.png'), 'data:image/jpeg;base64,AAAA');
  qc.setQueryData(qk.mods(id), []);
  qc.setQueryData(qk.contentFiles(id, 'shader'), []);
  qc.setQueryData(qk.packStatus(id), null);
  qc.setQueryData(qk.modIcon(id, 'a.jar', 1), 'data:image/png;base64,AAAA');
  qc.setQueryData(qk.continueTarget(id), null);
}

function cachedFor(qc: ReturnType<typeof createQueryClient>, id: string) {
  return qc
    .getQueryCache()
    .getAll()
    .filter((q) => q.queryKey.includes(id));
}

afterEach(() => clearMocks());

it('forgets a deleted instance, so a new one with its name never shows the old icon', async () => {
  const { qc, wrapper } = setup();
  seed(qc, 'sky-islands');
  seed(qc, 'creative-flat');
  const { result } = renderHook(() => useDeleteInstance(), { wrapper });
  await act(() => result.current.mutateAsync('sky-islands'));
  expect(qc.getQueryData(qk.instanceArt('sky-islands'))).toBeUndefined();
  expect(cachedFor(qc, 'sky-islands')).toEqual([]);
  // Other instances keep theirs.
  expect(qc.getQueryData(qk.instanceArt('creative-flat'))).toEqual(PICTURE);
});

it('forgets the old id of an instance whose folder moved with a rename', async () => {
  const { qc, wrapper } = setup();
  seed(qc, 'sky-islands');
  const { result } = renderHook(() => useRenameInstance(), { wrapper });
  const renamed = await act(() => result.current.mutateAsync({ id: 'sky-islands', name: 'Cloud Isles' }));
  expect(renamed.id).toBe('cloud-isles');
  expect(cachedFor(qc, 'sky-islands')).toEqual([]);
});

it('shows a background view at once and saves all of it together', async () => {
  const { qc, wrapper } = setup();
  qc.setQueryData(qk.instances, await (await import('../ipc/commands')).commands.instancesList());
  const cached = () => qc.getQueryData<InstanceListing>(qk.instances)!.instances.find((i) => i.id === 'sky-islands')!;
  const view = { focus: { x: 10, y: 20 }, zoom: 60, edges: 'pixel' } as const;
  const { result } = renderHook(() => ({ preview: usePreviewBannerView(), save: useSetBannerView() }), { wrapper });
  act(() => result.current.preview('sky-islands', view));
  expect([cached().bannerFocus, cached().bannerZoom, cached().bannerEdges]).toEqual([{ x: 10, y: 20 }, 60, 'pixel']);
  const saved = await act(() => result.current.save.mutateAsync({ id: 'sky-islands', view }));
  expect([saved.bannerFocus, saved.bannerZoom, saved.bannerEdges]).toEqual([{ x: 10, y: 20 }, 60, 'pixel']);
});
