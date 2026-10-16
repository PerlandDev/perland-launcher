import { renderHook } from '@testing-library/react';
import { mockIPC } from '@tauri-apps/api/mocks';
import { expect, it, vi } from 'vitest';
import { exitOf } from '../ipc/mock';
import { resetActivity, useActivity } from './activity';
import { contentProgressSnapshot } from './contentProgress';
import { handleEvent } from './handleEvent';
import { qk, queryClient } from './queries';
import { routeSnapshot } from './route';
import { runStore } from './runStore';
import { clearToasts, toastsSnapshot } from './toasts';

it('turns warnings into toasts and opens the drawer on a crash', () => {
  clearToasts();
  resetActivity();
  handleEvent({
    type: 'warning',
    instance: 'pack',
    error: { category: 'disk', code: 'log_write_stopped', message: 'x', fix: null, detail: null, params: {} },
  });
  expect(toastsSnapshot()).toHaveLength(1);
  handleEvent({
    type: 'run_state',
    instance: 'pack',
    state: 'idle',
    exit: exitOf({ code: 1 }),
    update: null,
  });
  const { result } = renderHook(() => useActivity());
  expect(result.current).toEqual({ open: true, focus: 'pack' });
  expect(runStore.run('pack').lastExit?.code).toBe(1);
});

it('leaves the drawer alone for a requested stop', () => {
  resetActivity();
  handleEvent({
    type: 'run_state',
    instance: 'pack',
    state: 'idle',
    exit: exitOf({ killed: true }),
    update: null,
  });
  const { result } = renderHook(() => useActivity());
  expect(result.current.open).toBe(false);
});

it('opens the drawer when a game fails to start', () => {
  resetActivity();
  const error = { category: 'game', code: 'java_start_failed', message: 'x', fix: null, detail: null, params: {} } as const;
  handleEvent({
    type: 'run_state',
    instance: 'pack',
    state: 'idle',
    exit: exitOf({ error }),
    update: null,
  });
  const { result } = renderHook(() => useActivity());
  expect(result.current).toEqual({ open: true, focus: 'pack' });
});

it('an ended run refreshes the instance and its pack queries', () => {
  const spy = vi.spyOn(queryClient, 'invalidateQueries');
  handleEvent({ type: 'run_state', instance: 'sky', state: 'idle', exit: exitOf({ code: 0 }), update: null });
  const keys = spy.mock.calls.map(([filters]) => filters?.queryKey);
  expect(keys).toContainEqual(qk.instances);
  expect(keys).toContainEqual(qk.packStatus('sky'));
  expect(keys).toContainEqual(qk.packContents('sky'));
  expect(keys).toContainEqual(qk.instanceArt('sky'));
  // A sync can change the pack's own resource packs and shaders too.
  expect(keys).toContainEqual(qk.contentFiles('sky', 'resource_pack'));
  expect(keys).toContainEqual(qk.contentFiles('sky', 'shader'));
  // The game's time is on disk before `idle` goes out.
  expect(keys).toContainEqual(qk.playtime);
  spy.mockRestore();
});

it('leaves the drawer closed when files need a manual download', () => {
  resetActivity();
  const error = { category: 'pack', code: 'manual_downloads_needed', message: 'x', fix: null, detail: null, params: { count: '1' } } as const;
  const manual = [{ path: 'mods/a.jar', name: 'A', fileName: 'a.jar', pageUrl: 'https://www.curseforge.com/projects/1',
    hashes: { sha1: 'a', sha256: null, sha512: null, md5: null, murmur2: null } }];
  handleEvent({ type: 'run_state', instance: 'sky', state: 'idle', exit: exitOf({ error, manual }), update: null });
  const { result } = renderHook(() => useActivity());
  expect(result.current.open).toBe(false);
});

it('opens New instance with a deep link once setup is done', async () => {
  mockIPC((cmd) => (cmd === 'deep_link_take' ? { kind: 'pack', url: 'https://x.example/pack.toml' } : null));
  queryClient.setQueryData(qk.settings, { version: 1, language: null, defaultMemoryMb: 4096, minimizeOnPlay: true, firstRunDone: false, railExpanded: false, iconShape: 'square' });
  handleEvent({ type: 'deep_link' });
  await new Promise((r) => setTimeout(r, 10));
  expect(routeSnapshot()).toEqual({ name: 'home', id: null }); // first run: the link waits
  queryClient.setQueryData(qk.settings, { version: 1, language: null, defaultMemoryMb: 4096, minimizeOnPlay: true, firstRunDone: true, railExpanded: false, iconShape: 'square' });
  handleEvent({ type: 'deep_link' });
  await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'newInstance', link: 'https://x.example/pack.toml' }));
  queryClient.clear();
});

it('sends install progress to its own store, not the run', () => {
  handleEvent({
    type: 'progress',
    instance: 'asalith-fields',
    task: { key: 'install_content', params: { name: 'Sodium' } },
    done: 1,
    total: 2,
    bytes: 10,
  });
  expect(runStore.run('asalith-fields').progress).toBeNull();
  expect(contentProgressSnapshot('asalith-fields')?.done).toBe(1);
});

it('opens Join server for a join deep link', async () => {
  const invite = `https://play.example.com/${'A'.repeat(43)}`;
  mockIPC((cmd) => (cmd === 'deep_link_take' ? { kind: 'join', invite } : null));
  queryClient.setQueryData(qk.settings, { version: 1, language: null, defaultMemoryMb: 4096, minimizeOnPlay: true, firstRunDone: true, railExpanded: false, iconShape: 'square' });
  handleEvent({ type: 'deep_link' });
  await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'joinServer', invite }));
  queryClient.clear();
});

it('an ended run refreshes a server instance status', () => {
  const spy = vi.spyOn(queryClient, 'invalidateQueries');
  handleEvent({ type: 'run_state', instance: 'survival', state: 'idle', exit: exitOf({ code: 0 }), update: null });
  expect(spy.mock.calls.map(([f]) => f?.queryKey)).toContainEqual(qk.serverStatus('survival'));
  spy.mockRestore();
});

it('keeps the latest launcher update state in the query cache', () => {
  handleEvent({ type: 'update', state: { state: 'ready', version: '0.2.0' } });
  expect(queryClient.getQueryData(qk.update)).toEqual({ state: 'ready', version: '0.2.0' });
});
