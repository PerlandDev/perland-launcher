import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { isNew, latestLine } from '../features/news/newsRules';
import { createMockBackend, installMock, stopMockRuns } from './mock';
import { MOCK_INVITE, MOCK_PUBLIC_SERVERS, MOCK_SERVER_ICON } from './mockServers';
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
  PublicServerPage,
  ServerNews,
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

  it('moves an instance to a new id when the name leaves its slug family, like the core', async () => {
    const { handle } = createMockBackend();
    const rename = (id: string, name: string) => handle('instance_rename', { id, name }) as Promise<Instance>;
    const ids = async () => ((await handle('instances_list')) as InstanceListing).instances.map((i) => i.id);
    // Same family (punctuation, case, or a numbered copy) keeps the id and the seed.
    expect(await rename('asalith-fields', 'Asalith  FIELDS!')).toMatchObject({ id: 'asalith-fields', artSeed: null });
    // A new slug moves the id, keeps its place in the order and pins the old id as the generated look.
    const before = await ids();
    expect(await rename('asalith-fields', 'Sky Base')).toMatchObject({ id: 'sky-base', name: 'Sky Base', artSeed: 'asalith-fields' });
    expect(await ids()).toEqual(before.map((id) => (id === 'asalith-fields' ? 'sky-base' : id)));
    // A taken slug gets a number.
    expect((await rename('sky-base', 'Creative Flat')).id).toBe('creative-flat-2');
    expect((await rename('creative-flat-2', 'creative flat')).id).toBe('creative-flat-2');
    await expect(rename('asalith-fields', 'x')).rejects.toMatchObject({ code: 'instance_not_found' });
  });

  it('changes an instance image: presets, pack art and uploads', async () => {
    const { handle } = createMockBackend();
    const art = (id: string) => handle('instance_art', { id }) as Promise<{ icon: string | null; banner: string | null }>;
    expect((await art('sky-islands')).icon).toMatch(/^data:image\/svg/);
    const generated = (await handle('instance_art_set', { id: 'sky-islands', choice: { kind: 'generated', seed: 'dunes' } })) as Instance;
    expect([generated.art, generated.artSeed]).toEqual(['generated', 'dunes']);
    // The background changed; the pack's icon is pinned and stays.
    expect(generated.icon).toBe('auto');
    const pinned = await art('sky-islands');
    expect(pinned.icon).toMatch(/^data:image\/svg/);
    expect(pinned.banner).toBeNull();
    const custom = (await handle('instance_art_set', { id: 'sky-islands', choice: { kind: 'upload', path: 'C:/pics/Me.JPG' } })) as Instance;
    expect(custom.art).toBe('custom');
    const mine = await art('sky-islands');
    expect(mine.banner).toMatch(/^data:image\/jpeg;base64,/);
    expect(mine.icon).toMatch(/^data:image\/svg/);
    const back = (await handle('instance_art_set', { id: 'sky-islands', choice: { kind: 'pack' } })) as Instance;
    expect(back.art).toBe('auto');
    expect((await art('sky-islands')).icon).toMatch(/^data:image\/svg/);
  });

  it('keeps the icon apart from the background', async () => {
    const { handle, state } = createMockBackend();
    const art = (id: string) => handle('instance_art', { id }) as Promise<{ icon: string | null; banner: string | null }>;
    const mine = (await handle('instance_icon_set', { id: 'sky-islands', choice: { kind: 'upload', path: 'C:/pics/me.webp' } })) as Instance;
    expect(mine.icon).toBe('custom');
    await handle('instance_art_set', { id: 'sky-islands', choice: { kind: 'upload', path: 'C:/pics/view.png' } });
    expect(await art('sky-islands')).toEqual({
      icon: expect.stringMatching(/^data:image\/webp;base64,/),
      banner: expect.stringMatching(/^data:image\/png;base64,/),
    });
    const generated = (await handle('instance_icon_set', { id: 'sky-islands', choice: { kind: 'generated' } })) as Instance;
    expect(generated.icon).toBe('generated');
    expect((await art('sky-islands')).icon).toBeNull();
    const pack = (await handle('instance_icon_set', { id: 'sky-islands', choice: { kind: 'pack' } })) as Instance;
    expect(pack.icon).toBe('auto');
    expect((await art('sky-islands')).icon).toMatch(/^data:image\/svg/);
    await expect(handle('instance_icon_set', { id: 'sky-islands', choice: { kind: 'upload', path: 'C:/pics/invalid.png' } })).rejects.toMatchObject({
      code: 'art_file_invalid',
    });
    const plain = state.listing.instances.find((i) => i.source.kind === 'none')!;
    await expect(handle('instance_icon_set', { id: plain.id, choice: { kind: 'pack' } })).rejects.toMatchObject({ code: 'no_pack' });
  });

  it('lets a round-1 upload fill both places until the background changes', async () => {
    const { handle, state } = createMockBackend();
    const art = (id: string) => handle('instance_art', { id }) as Promise<{ icon: string | null; banner: string | null }>;
    await handle('instance_art_set', { id: 'sky-islands', choice: { kind: 'upload', path: 'C:/pics/old.png' } });
    // As a round-1 instance.json loads: a custom picture and no icon chosen.
    state.listing.instances = state.listing.instances.map((i) => (i.id === 'sky-islands' ? { ...i, icon: null } : i));
    const legacy = await art('sky-islands');
    expect(legacy.icon).toMatch(/^data:image\/png;base64,/);
    expect(legacy.banner).toBe(legacy.icon);
    const set = (await handle('instance_art_set', { id: 'sky-islands', choice: { kind: 'generated', seed: 'dunes' } })) as Instance;
    expect(set.icon).toBe('custom');
    expect(await art('sky-islands')).toEqual({ icon: legacy.icon, banner: null });
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

  it('the awaiting option rejects with microsoft_awaiting_approval and then accountReady says so', async () => {
    const { handle } = createMockBackend({ beta: true, microsoft: 'awaiting' });
    expect(await handle('account_ready')).toEqual({ ready: false, awaitingApproval: false, offlineNotice: true });
    await expect(handle('account_sign_in_microsoft', { language: 'en' })).rejects.toMatchObject({ code: 'microsoft_awaiting_approval' });
    expect(await handle('account_ready')).toMatchObject({ ready: false, awaitingApproval: true });
  }, 10_000);

  it('signs in with Microsoft as Alex, ready to play', async () => {
    const { handle } = createMockBackend({ beta: true });
    const file = (await handle('account_sign_in_microsoft', { language: 'en' })) as AccountsFile;
    expect(file.accounts).toEqual([{ provider: 'microsoft', username: 'Alex', uuid: expect.any(String) }]);
    expect(file.active).toBe(file.accounts[0]!.uuid);
    expect(await handle('account_ready')).toEqual({ ready: true, awaitingApproval: false, offlineNotice: true });
    await handle('account_dismiss_offline_notice');
    expect(await handle('account_ready')).toMatchObject({ offlineNotice: false });
    expect(((await handle('accounts_list')) as AccountsFile).offlineNoticeDismissed).toBe(true);
  }, 10_000);

  it('answers a canceled Microsoft sign-in with null', async () => {
    const { handle } = createMockBackend({ beta: true });
    const signing = handle('account_sign_in_microsoft', { language: 'en' });
    await handle('account_sign_in_cancel');
    expect(await signing).toBeNull();
    expect(((await handle('accounts_list')) as AccountsFile).accounts).toEqual([]);
  });

  it('fails a Microsoft sign-in with the matching code', async () => {
    await expect(createMockBackend({ microsoft: 'notConfigured' }).handle('account_sign_in_microsoft', { language: 'en' })).rejects.toMatchObject({
      code: 'microsoft_not_configured',
    });
    await expect(createMockBackend({ microsoft: 'notOwned' }).handle('account_sign_in_microsoft', { language: 'en' })).rejects.toMatchObject({
      code: 'minecraft_not_owned',
    });
  }, 10_000);

  it('hides offline accounts in a beta build and keeps them in dev', async () => {
    const beta = (await createMockBackend({ beta: true }).handle('accounts_list')) as AccountsFile;
    expect(beta.accounts).toEqual([]);
    expect(beta.active).toBeNull();
    const dev = createMockBackend();
    expect(((await dev.handle('accounts_list')) as AccountsFile).accounts).toHaveLength(2);
    expect(await dev.handle('account_ready')).toEqual({ ready: false, awaitingApproval: false, offlineNotice: false });
    const signedIn = (await createMockBackend({ beta: true, microsoft: 'signedIn' }).handle('accounts_list')) as AccountsFile;
    expect(signedIn.accounts.map((a) => [a.provider, a.username])).toEqual([['microsoft', 'Alex']]);
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

  it('unlinks a server instance into an ordinary one, and refuses any other', async () => {
    const { handle, state } = createMockBackend({ server: 'online' });
    const plain = (await handle('server_unlink', { id: 'survival' })) as Instance;
    expect(plain.source).toEqual({ kind: 'none' });
    expect(plain.server).toBeNull();
    expect(state.listing.instances.find((i) => i.id === 'survival')).toEqual(plain);
    await expect(handle('server_unlink', { id: 'survival' })).rejects.toMatchObject({ code: 'no_pack' });
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

describe('mock public server list', () => {
  afterEach(() => stopMockRuns());

  const query = (over: Record<string, unknown> = {}) => ({
    query: { sort: 'players', modded: null, minecraft: null, hasRoom: false, category: null, language: null, region: null, offset: 0, ...over },
  });
  const list = async (over: Record<string, unknown> = {}) =>
    (await createMockBackend().handle('servers_list', query(over))) as PublicServerPage;

  it('has eight cards with fake tokens that cover every state', () => {
    expect(MOCK_PUBLIC_SERVERS).toHaveLength(8);
    for (const card of MOCK_PUBLIC_SERVERS) expect(card.token).toMatch(/^([A-Za-z])\1{42}$/);
    const has = (f: (c: (typeof MOCK_PUBLIC_SERVERS)[number]) => boolean) => MOCK_PUBLIC_SERVERS.some(f);
    expect(has((c) => c.status === 'online' && (c.players?.online ?? 0) > 0)).toBe(true);
    expect(has((c) => c.players !== null && c.players.online === c.players.max)).toBe(true);
    expect(has((c) => c.status === 'sleeping')).toBe(true);
    expect(has((c) => c.status === 'starting')).toBe(true);
    expect(has((c) => c.whitelisted)).toBe(true);
    expect(has((c) => !c.modded)).toBe(true);
    expect(has((c) => !c.joinable && c.address !== null)).toBe(true);
    expect(new Set(MOCK_PUBLIC_SERVERS.map((c) => c.game.version)).size).toBeGreaterThanOrEqual(2);
    expect(new Set(MOCK_PUBLIC_SERVERS.map((c) => c.language))).toEqual(new Set(['en', 'es']));
  });

  it('sorts by players, name and newest', async () => {
    const online = (page: PublicServerPage) => page.servers.map((s) => s.players?.online ?? 0);
    const byPlayers = online(await list({ sort: 'players' }));
    expect(byPlayers).toEqual([...byPlayers].sort((a, b) => b - a));
    const names = (await list({ sort: 'name' })).servers.map((s) => s.name);
    expect(names).toEqual([...names].sort((a, b) => a.localeCompare(b)));
    const newest = (await list({ sort: 'newest' })).servers.map((s) => s.name);
    expect(newest).not.toEqual(names);
    expect([...newest].sort()).toEqual([...names].sort());
  });

  it('applies modded, minecraft, hasRoom, category, language and region', async () => {
    const modded = await list({ modded: true });
    expect(modded.servers.length).toBeGreaterThan(0);
    expect(modded.servers.every((s) => s.modded)).toBe(true);
    expect((await list({ modded: false })).servers.every((s) => !s.modded)).toBe(true);
    const mc = MOCK_PUBLIC_SERVERS[0]!.game.version!;
    const versioned = await list({ minecraft: mc });
    expect(versioned.servers.length).toBeGreaterThan(0);
    expect(versioned.servers.every((s) => s.game.version === mc)).toBe(true);
    const room = await list({ hasRoom: true });
    expect(room.servers.length).toBeLessThan(MOCK_PUBLIC_SERVERS.length);
    expect(room.servers.every((s) => !s.players || s.players.max === null || s.players.online < s.players.max)).toBe(true);
    const creative = await list({ category: 'creative' });
    expect(creative.servers.length).toBeGreaterThan(0);
    expect(creative.servers.every((s) => s.categories.includes('creative'))).toBe(true);
    const es = await list({ language: 'es' });
    expect(es.servers.length).toBeGreaterThan(0);
    expect(es.servers.every((s) => s.language === 'es')).toBe(true);
    const eu = await list({ region: 'eu' });
    expect(eu.servers.every((s) => s.region === 'eu')).toBe(true);
    const both = await list({ modded: false, language: 'es' });
    expect(both.total).toBe(both.servers.length);
  });

  it('keeps facets over the whole set and total over the filtered one', async () => {
    const all = await list();
    expect(all.total).toBe(8);
    const filtered = await list({ language: 'es' });
    expect(filtered.total).toBeLessThan(8);
    expect(filtered.facets).toEqual(all.facets);
    expect(all.facets.languages).toEqual(['en', 'es']);
    expect(all.facets.minecraftVersions.length).toBeGreaterThanOrEqual(2);
    expect(all.facets.categories).toContain('creative');
    const past = await list({ offset: 50 });
    expect(past.servers).toEqual([]);
    expect(past.total).toBe(8);
  });

  it('marks the card whose invite an instance already joins', async () => {
    const { handle } = createMockBackend({ server: 'online' });
    const page = (await handle('servers_list', query())) as PublicServerPage;
    expect(page.servers.find((s) => s.invite === MOCK_INVITE)?.instance).toBe('survival');
    expect(page.servers.filter((s) => s.instance !== null)).toHaveLength(1);
  });

  it('answers a server icon only for a card with an icon version', async () => {
    const { handle } = createMockBackend();
    const withIcon = MOCK_PUBLIC_SERVERS.find((c) => c.iconVersion)!;
    expect(await handle('servers_icon', { token: withIcon.token, iconVersion: withIcon.iconVersion })).toMatch(/^data:image\//);
    expect(await handle('servers_icon', { token: 'Z'.repeat(43), iconVersion: 'x' })).toBeNull();
  });

  it('answers server_list_closed under the closed scenario', async () => {
    const closed = createMockBackend({ server: 'closed' });
    await expect(closed.handle('servers_list', query())).rejects.toMatchObject({ category: 'network', code: 'server_list_closed' });
  });

  async function runPhases(card: (typeof MOCK_PUBLIC_SERVERS)[number]) {
    vi.useFakeTimers();
    try {
      const backend = installMock();
      const added = (await backend.handle('join_add', { link: card.invite })) as Instance;
      expect(added.source).toMatchObject({ kind: 'perland' });
      expect(added.name).toBe(card.name);
      await backend.handle('play', { id: added.id, join: true });
      const seen: string[] = [];
      for (let i = 0; i < 400; i++) {
        await vi.advanceTimersByTimeAsync(50);
        const state = backend.state.runs.get(added.id)?.state;
        if (state && seen.at(-1) !== state) seen.push(state);
        if (state === 'running') break;
      }
      return { seen, id: added.id, backend };
    } finally {
      stopMockRuns();
      clearMocks();
      vi.useRealTimers();
    }
  }

  it('wakes a sleeping server between preparing and running', async () => {
    const sleeping = MOCK_PUBLIC_SERVERS.find((c) => c.status === 'sleeping')!;
    const { seen } = await runPhases(sleeping);
    expect(seen.slice(-3)).toEqual(['preparing', 'waking', 'running']);
  });

  it('runs an online server without waking it', async () => {
    const online = MOCK_PUBLIC_SERVERS.find((c) => c.status === 'online' && c.joinable)!;
    const { seen } = await runPhases(online);
    expect(seen).not.toContain('waking');
    expect(seen.at(-1)).toBe('running');
  });

  it('refuses to add a card that cannot be joined', async () => {
    const closed = MOCK_PUBLIC_SERVERS.find((c) => !c.joinable)!;
    expect(closed.invite).toBeNull();
    await expect(createMockBackend().handle('join_add', { link: `https://play.example.com/${'Q'.repeat(43)}` })).rejects.toMatchObject({
      code: 'invite_gone',
    });
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

describe('mock server news', () => {
  afterEach(() => stopMockRuns());

  const news = async (invite: string, over: { before?: string | null; limit?: number } = {}, options: Parameters<typeof createMockBackend>[0] = {}) =>
    (await createMockBackend(options).handle('server_news', { invite, before: null, limit: 20, ...over })) as ServerNews | null;

  it('gives the seeded instance an upcoming event, an update with a picture, a modpack post and a body with markup', async () => {
    const page = (await news(MOCK_INVITE, {}, { server: 'online' }))!;
    const now = new Date();
    expect(page.posts.map((p) => p.type)).toEqual(expect.arrayContaining(['event', 'update', 'modpack']));
    const e = page.posts.find((p) => p.type === 'event')!;
    expect(e.title).toBe('Build contest');
    expect(new Date(e.startsAt!).getTime()).toBeGreaterThan(now.getTime());
    expect(page.posts.some((p) => p.imageVersion !== null)).toBe(true);
    expect(page.posts.some((p) => (p.modpack?.added.count ?? 0) > 0)).toBe(true);
    expect(page.posts.some((p) => p.body === '<b>bold</b>')).toBe(true);
    expect(latestLine(page.posts, now)?.id).toBe(e.id);
  });

  it('shows the dot for the seeded instance until its news was opened', async () => {
    const backend = createMockBackend({ server: 'online' });
    const ask = async () => (await backend.handle('server_news', { invite: MOCK_INVITE, before: null, limit: 20 })) as ServerNews;
    const first = await ask();
    expect(first.seen).not.toBeNull();
    expect(isNew(first.posts, first.seen, new Date())).toBe(true);
    const newest = first.posts.map((p) => p.createdAt).sort().at(-1)!;
    expect(await backend.handle('server_news_opened', { invite: MOCK_INVITE, newest })).toBeNull();
    const after = await ask();
    expect(after.seen).toBe(newest);
    expect(isNew(after.posts, after.seen, new Date())).toBe(false);
  });

  it('gives the seeded instance more than one page of news', async () => {
    const first = (await news(MOCK_INVITE, {}, { server: 'online' }))!;
    expect(first.posts).toHaveLength(20);
    expect(first.nextBefore).toBe('20');
    const second = (await news(MOCK_INVITE, { before: first.nextBefore }, { server: 'online' }))!;
    expect(second.posts).toHaveLength(4);
    expect(second.nextBefore).toBeNull();
  });

  it('pages with nextBefore', async () => {
    const backend = createMockBackend({ server: 'online' });
    const first = (await backend.handle('server_news', { invite: MOCK_INVITE, before: null, limit: 2 })) as ServerNews;
    expect(first.posts).toHaveLength(2);
    expect(first.nextBefore).not.toBeNull();
    const second = (await backend.handle('server_news', { invite: MOCK_INVITE, before: first.nextBefore, limit: 50 })) as ServerNews;
    expect(second.posts.length).toBeGreaterThan(0);
    expect(second.nextBefore).toBeNull();
    expect(new Set([...first.posts, ...second.posts].map((p) => p.id)).size).toBe(first.posts.length + second.posts.length);
  });

  it('answers the public cards, and one card has none', async () => {
    const withInvite = MOCK_PUBLIC_SERVERS.filter((c) => c.invite !== null);
    const pages = await Promise.all(withInvite.map((c) => news(c.invite!)));
    expect(pages.filter((p) => p && p.posts.length > 0).length).toBeGreaterThanOrEqual(withInvite.length - 2);
    expect(pages.some((p) => p?.posts.length === 0)).toBe(true);
  });

  it('answers null for the news-off scenario and for a link it does not know', async () => {
    expect(await news(MOCK_INVITE, {}, { server: 'news-off' })).toBeNull();
    expect(await news(`https://play.example.com/${'Z'.repeat(43)}`)).toBeNull();
  });

  it('serves a post picture only for its own version', async () => {
    const backend = createMockBackend({ server: 'online' });
    const page = (await backend.handle('server_news', { invite: MOCK_INVITE, before: null, limit: 20 })) as ServerNews;
    const withImage = page.posts.find((p) => p.imageVersion !== null)!;
    expect(await backend.handle('news_image', { invite: MOCK_INVITE, postId: withImage.id, imageVersion: withImage.imageVersion })).toBe(MOCK_SERVER_ICON);
    expect(await backend.handle('news_image', { invite: MOCK_INVITE, postId: withImage.id, imageVersion: 'old' })).toBeNull();
  });
});
