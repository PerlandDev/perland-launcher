import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createMockBackend, installMock, stopMockRuns } from './mock';
import { MOCK_INVITE } from './mockServers';
import type {
  AccountsFile,
  InstallPlanView,
  InstallResult,
  Instance,
  InstanceListing,
  JoinPreview,
  LoaderVersion,
  ManualProgress,
  PackContents,
  PackStatus,
  RunSnapshot,
  SearchPage,
  ServerStatusView,
  Settings,
} from './schema';

describe('mock backend', () => {
  it('serves the contract fixtures', async () => {
    const { handle } = createMockBackend();
    const listing = (await handle('instances_list')) as InstanceListing;
    expect(listing.instances.map((i) => i.id)).toEqual(['asalith-fields', 'creative-flat', 'sky-islands']);
    expect(listing.damaged).toHaveLength(1);
    const settings = (await handle('settings_get')) as Settings;
    expect(settings.firstRunDone).toBe(true);
  });

  it('puts new instances on top and keeps a reorder', async () => {
    const { handle } = createMockBackend();
    const ids = async () => ((await handle('instances_list')) as InstanceListing).instances.map((i) => i.id);
    await handle('instance_create', { request: { name: 'Zed', minecraft: '1.21.1', loader: null } });
    expect(await ids()).toEqual(['zed', 'asalith-fields', 'creative-flat', 'sky-islands']);
    const reordered = (await handle('instance_reorder', { ids: ['sky-islands', 'zed', 'asalith-fields', 'gone'] })) as InstanceListing;
    // Ids left out (and unknown ones) are dropped from the order; the left-out instance is unlisted, so on top.
    expect(reordered.instances.map((i) => i.id)).toEqual(['creative-flat', 'sky-islands', 'zed', 'asalith-fields']);
    expect(await ids()).toEqual(['creative-flat', 'sky-islands', 'zed', 'asalith-fields']);
  });

  it('starts empty for first run', async () => {
    const { handle } = createMockBackend({ firstRun: true });
    expect(((await handle('instances_list')) as InstanceListing).instances).toEqual([]);
    expect(((await handle('accounts_list')) as AccountsFile).accounts).toEqual([]);
    expect(((await handle('settings_get')) as Settings).firstRunDone).toBe(false);
  });

  it('creates instances and toggles mods like the real backend', async () => {
    const { handle } = createMockBackend();
    const created = (await handle('instance_create', {
      request: { name: 'New One', minecraft: '1.21.1', loader: null },
    })) as { id: string };
    expect(created.id).toBe('new-one');
    const mods = (await handle('mod_set_enabled', {
      id: 'asalith-fields',
      file: 'zoomify-2.11.2.jar.disabled',
      enabled: true,
    })) as { fileName: string; enabled: boolean }[];
    expect(mods.find((m) => m.fileName === 'zoomify-2.11.2.jar')?.enabled).toBe(true);
  });

  it('returns fresh objects like real IPC, never live state', async () => {
    const { handle } = createMockBackend();
    const first = (await handle('instances_list')) as InstanceListing;
    const second = (await handle('instances_list')) as InstanceListing;
    expect(second).toEqual(first);
    expect(second).not.toBe(first);

    const before = (await handle('accounts_list')) as AccountsFile;
    const activeBefore = before.active;
    const other = before.accounts.find((a) => a.uuid !== activeBefore);
    const after = (await handle('account_set_active', { uuid: other?.uuid })) as AccountsFile;
    expect(after.active).toBe(other?.uuid);
    expect(before.active).toBe(activeBefore);
    expect(((await handle('accounts_list')) as AccountsFile).active).toBe(other?.uuid);

    const renamed = (await handle('instance_rename', { id: 'asalith-fields', name: 'Renamed' })) as { name: string };
    expect(renamed.name).toBe('Renamed');
    expect(first.instances.find((i) => i.id === 'asalith-fields')?.name).not.toBe('Renamed');
  });

  it('changes an instance image: presets, pack art and uploads', async () => {
    const { handle } = createMockBackend();
    const art = (id: string) => handle('instance_art', { id }) as Promise<{ icon: string | null; banner: string | null }>;
    expect((await art('sky-islands')).icon).toMatch(/^data:image\/svg/);
    const generated = (await handle('instance_art_set', { id: 'sky-islands', choice: { kind: 'generated', seed: 'dunes' } })) as Instance;
    expect([generated.art, generated.artSeed]).toEqual(['generated', 'dunes']);
    expect(await art('sky-islands')).toEqual({ icon: null, banner: null });
    const custom = (await handle('instance_art_set', { id: 'sky-islands', choice: { kind: 'upload', path: 'C:/pics/Me.JPG' } })) as Instance;
    expect(custom.art).toBe('custom');
    const mine = await art('sky-islands');
    expect(mine.icon).toMatch(/^data:image\/jpeg;base64,/);
    expect(mine.banner).toBe(mine.icon);
    const back = (await handle('instance_art_set', { id: 'sky-islands', choice: { kind: 'pack' } })) as Instance;
    expect(back.art).toBe('auto');
    expect((await art('sky-islands')).icon).toMatch(/^data:image\/svg/);
  });

  it('refuses a bad upload and a pack choice without a pack', async () => {
    const { handle } = createMockBackend();
    for (const path of ['C:/pics/invalid.png', 'C:/pics/clip.gif']) {
      await expect(handle('instance_art_set', { id: 'sky-islands', choice: { kind: 'upload', path } })).rejects.toMatchObject({
        code: 'art_file_invalid',
      });
    }
    const listing = (await handle('instances_list')) as InstanceListing;
    expect(listing.instances.find((i) => i.id === 'sky-islands')?.art).toBe('auto');
    const plain = listing.instances.find((i) => i.source.kind === 'none')!;
    await expect(handle('instance_art_set', { id: plain.id, choice: { kind: 'pack' } })).rejects.toMatchObject({ code: 'no_pack' });
  });

  it('rejects offline accounts in a release-like build', async () => {
    const { handle } = createMockBackend({ release: true });
    await expect(handle('account_add_offline', { name: 'Steve' })).rejects.toMatchObject({ code: 'offline_locked' });
  });

  it('scripts a pack run that asks, updates and plays', async () => {
    const backend = installMock();
    await backend.handle('play', { id: 'sky-islands' });
    await vi.waitFor(() => expect(backend.state.runs.get('sky-islands')?.state).toBe('asking'));
    await backend.handle('play_answer', { id: 'sky-islands', update: true });
    await vi.waitFor(() => expect(backend.state.runs.get('sky-islands')?.state).toBe('running'), { timeout: 10_000 });
    const status = (await backend.handle('pack_status', { id: 'sky-islands' })) as PackStatus;
    expect(status).toMatchObject({ installedVersion: '1.0.4', update: null });
    stopMockRuns();
  }, 15_000); // the scripted run takes ~2.6 s of real timers; slow runners need room

  it('finds manual downloads one poll at a time', async () => {
    const { handle } = createMockBackend({ manual: true });
    const first = (await handle('manual_import', { id: 'sky-islands' })) as ManualProgress;
    const second = (await handle('manual_import', { id: 'sky-islands' })) as ManualProgress;
    expect([first.found, second.found, second.total]).toEqual([1, 2, 2]);
    expect(second.remaining).toEqual([]);
  });

  it('serves Forge versions for Forge', async () => {
    const { handle } = createMockBackend();
    const forge = (await handle('catalog_loaders', { kind: 'forge', mc: '1.20.1' })) as LoaderVersion[];
    expect(forge[0]?.version).toBe('47.4.22');
  });

  it("keeps a pack's additions in step with its mods folder", async () => {
    const { handle } = createMockBackend();
    const additions = async () => ((await handle('pack_contents', { id: 'sky-islands' })) as PackContents).additions;
    await handle('mod_set_enabled', { id: 'sky-islands', file: 'zoomify-2.11.2.jar', enabled: false });
    expect(await additions()).toEqual([{ fileName: 'zoomify-2.11.2.jar.disabled', enabled: false, size: 524288 }]);
    await handle('mods_add', { id: 'sky-islands', paths: ['C:/Downloads/lithium.jar'] });
    expect((await additions()).map((m) => m.fileName)).toEqual(['lithium.jar', 'zoomify-2.11.2.jar.disabled']);
    await handle('mod_remove', { id: 'sky-islands', file: 'zoomify-2.11.2.jar.disabled' });
    expect((await additions()).map((m) => m.fileName)).toEqual(['lithium.jar']);
  });

  it('refuses links that are not https or modrinth', async () => {
    const { handle } = createMockBackend();
    await expect(handle('pack_add', { request: { kind: 'link', url: 'http://evil.example/pack.toml' } })).rejects.toMatchObject({
      code: 'pack_link_invalid',
    });
  });
});

describe('scripted runs', () => {
  it('stop for good when a test tears the backend down', async () => {
    vi.useFakeTimers();
    try {
      const emitted: unknown[] = [];
      mockIPC((cmd, args) => {
        if (cmd === 'plugin:event|emit') emitted.push(args);
        return null;
      });
      const { handle } = createMockBackend();
      await handle('play', { id: 'asalith-fields' });
      await vi.advanceTimersByTimeAsync(5_000); // queued → preparing → running
      expect(emitted.length).toBeGreaterThan(0);
      stopMockRuns();
      emitted.length = 0;
      await vi.advanceTimersByTimeAsync(5_000);
      expect(emitted).toEqual([]);
    } finally {
      clearMocks();
      vi.useRealTimers();
    }
  });

  it('never ends a stopped pack run twice', async () => {
    vi.useFakeTimers();
    try {
      const idles: unknown[] = [];
      mockIPC((cmd, args) => {
        const payload = (args as { payload?: { type?: string; state?: string; instance?: string } } | undefined)?.payload;
        if (cmd === 'plugin:event|emit' && payload?.type === 'run_state' && payload.state === 'idle') idles.push(payload);
        return null;
      });
      const { handle } = createMockBackend({ manual: true, noUpdate: true });
      await handle('play', { id: 'sky-islands' });
      await vi.advanceTimersByTimeAsync(1_830); // inside the last wait of the sync loop
      await handle('stop', { id: 'sky-islands' });
      await vi.advanceTimersByTimeAsync(500);
      expect(idles).toHaveLength(1);
      expect(idles[0]).toMatchObject({ exit: { cancelled: true, error: null, manual: [] } });
    } finally {
      stopMockRuns();
      clearMocks();
      vi.useRealTimers();
    }
  });

  it('reports the pending offer while asking, like run_states', async () => {
    vi.useFakeTimers();
    try {
      mockIPC(() => null);
      const { handle } = createMockBackend();
      const offer = ((await handle('pack_status', { id: 'sky-islands' })) as PackStatus).update;
      await handle('play', { id: 'sky-islands' });
      await vi.advanceTimersByTimeAsync(500); // checked; now asking
      expect((await handle('run_states')) as RunSnapshot[]).toEqual([{ instance: 'sky-islands', state: 'asking', update: offer }]);
    } finally {
      stopMockRuns();
      clearMocks();
      vi.useRealTimers();
    }
  });

  it('stops a sync through stopping, and an update-only one says so', async () => {
    vi.useFakeTimers();
    try {
      const states: { state?: string; exit?: unknown }[] = [];
      mockIPC((cmd, args) => {
        const payload = (args as { payload?: { type?: string; state?: string; exit?: unknown } } | undefined)?.payload;
        if (cmd === 'plugin:event|emit' && payload?.type === 'run_state') states.push(payload);
        return null;
      });
      const { handle } = createMockBackend();
      await handle('pack_update', { id: 'sky-islands' });
      await vi.advanceTimersByTimeAsync(800); // checking 400 ms, queued 200 ms, then syncing
      expect(states.at(-1)?.state).toBe('syncing');
      await handle('stop', { id: 'sky-islands' });
      expect(states.at(-1)?.state).toBe('stopping');
      await vi.advanceTimersByTimeAsync(1_000);
      const idles = states.filter((s) => s.state === 'idle');
      expect(idles).toHaveLength(1);
      expect(idles[0]).toMatchObject({ exit: { cancelled: true, updateOnly: true } });
    } finally {
      stopMockRuns();
      clearMocks();
      vi.useRealTimers();
    }
  });

  it('answers each server scenario', async () => {
    const online = createMockBackend({ server: 'online' });
    expect((await online.handle('join_preview', { link: MOCK_INVITE })) as JoinPreview).toMatchObject({
      name: 'survival',
      existing: 'survival',
      whitelisted: false,
    });
    expect(((await online.handle('server_status', { id: 'survival' })) as ServerStatusView).status).toBe('online');
    const preview = async (server: 'vanilla' | 'pending' | 'whitelisted') =>
      (await createMockBackend({ server }).handle('join_preview', { link: MOCK_INVITE })) as JoinPreview;
    expect((await preview('vanilla')).pack).toBeNull();
    expect((await preview('pending')).pack?.state).toBe('pending');
    expect((await preview('whitelisted')).whitelisted).toBe(true);
    await expect(createMockBackend({ server: 'network' }).handle('join_preview', { link: MOCK_INVITE })).rejects.toMatchObject({
      code: 'invite_unsupported',
      params: { address: 'play.example.com:25577' },
    });
    await expect(createMockBackend({ server: 'modded-no-pack' }).handle('join_preview', { link: MOCK_INVITE })).rejects.toMatchObject({
      code: 'invite_unsupported',
    });
    const gone = createMockBackend({ server: 'gone' });
    await expect(gone.handle('join_preview', { link: MOCK_INVITE })).rejects.toMatchObject({ code: 'invite_gone' });
    expect(((await gone.handle('server_status', { id: 'survival' })) as ServerStatusView).gone).toBe(true);
    await expect(online.handle('join_preview', { link: 'https://play.example.com/short' })).rejects.toMatchObject({
      code: 'invite_link_invalid',
    });
  });

  it('adds a server once and moves it to a new link', async () => {
    const { handle, state } = createMockBackend({ server: 'online' });
    const other = `https://play.example.com/${'B'.repeat(43)}`;
    const added = (await handle('join_add', { link: other })) as Instance;
    expect(((await handle('join_add', { link: ` ${other}/ ` })) as Instance).id).toBe(added.id);
    expect(state.listing.instances.filter((i) => i.source.kind === 'perland')).toHaveLength(2);
    const moved = (await handle('server_relink', { id: 'survival', link: other })) as Instance;
    expect(moved.source).toMatchObject({ kind: 'perland', invite: other });
  });

  it('hands an invite link over as a join deep link, and any other link as a pack', async () => {
    const join = createMockBackend({ link: ` ${MOCK_INVITE}/ ` });
    expect(await join.handle('deep_link_take')).toEqual({ kind: 'join', invite: MOCK_INVITE });
    expect(await join.handle('deep_link_take')).toBeNull();
    const pack = createMockBackend({ link: 'https://packs.example/a/pack.toml' });
    expect(await pack.handle('deep_link_take')).toEqual({ kind: 'pack', url: 'https://packs.example/a/pack.toml' });
  });

  it('scripts a pack the server is still building', async () => {
    vi.useFakeTimers();
    try {
      const backend = installMock({ server: 'pending' });
      await backend.handle('play', { id: 'survival' });
      expect(backend.state.runs.get('survival')?.state).toBe('checking');
      await vi.advanceTimersByTimeAsync(1_500); // still waiting on the server's pack
      expect(backend.state.runs.get('survival')?.state).toBe('checking');
      await vi.advanceTimersByTimeAsync(4_000); // 2 s of waiting, then queued, preparing and running
      expect(backend.state.runs.get('survival')?.state).toBe('running');
    } finally {
      stopMockRuns();
      clearMocks();
      vi.useRealTimers();
    }
  });
});

describe('mock browse backend', () => {
  afterEach(() => stopMockRuns());

  it('pages search results and filters by kind and query', async () => {
    const { handle } = createMockBackend();
    const base = { query: '', kind: 'mod', instance: null, minecraft: null, loader: null, categories: [], sort: 'downloads', offset: 0 };
    const first = (await handle('browse_search', { request: base })) as SearchPage;
    expect(first.hits).toHaveLength(20);
    expect(first.total).toBeGreaterThan(20);
    const second = (await handle('browse_search', { request: { ...base, offset: 20 } })) as SearchPage;
    expect(second.offset).toBe(20);
    const sodium = (await handle('browse_search', { request: { ...base, query: 'sodium' } })) as SearchPage;
    expect(sodium.hits.map((h) => h.title)).toEqual(['Sodium']);
    await expect(handle('browse_search', { request: { ...base, query: 'offline' } })).rejects.toMatchObject({ code: 'host_unreachable' });
    await expect(handle('browse_search', { request: { ...base, query: 'busy' } })).rejects.toMatchObject({
      code: 'browse_rate_limited',
      params: { seconds: '3' },
    });
  });

  it('plans with dependencies and installs once', async () => {
    const { handle } = createMockBackend();
    const view = (await handle('browse_plan', {
      request: { project: 'AANobbMI', version: null, target: { kind: 'instance', id: 'asalith-fields' } },
    })) as InstallPlanView;
    expect(view.items.map((i) => i.title)).toEqual(['Sodium']);
    expect(view.skipped.map((s) => s.title)).toEqual(['Fabric API']);
    const result = (await handle('browse_install', { planId: view.planId })) as InstallResult;
    expect(result.instance.id).toBe('asalith-fields');
    expect(await handle('browse_installed', { id: 'asalith-fields' })).toContain('AANobbMI');
    await expect(handle('browse_install', { planId: view.planId })).rejects.toMatchObject({ code: 'content_plan_expired' });
  });

  it('turns a modrinth pack link into an instance named after the project', async () => {
    const { handle } = createMockBackend();
    const created = (await handle('pack_add', { request: { kind: 'link', url: 'modrinth:1KVo5zza@1KVo5zza-v1' } })) as Instance;
    expect(created.name).toBe('Fabulously Optimized');
  });
});
