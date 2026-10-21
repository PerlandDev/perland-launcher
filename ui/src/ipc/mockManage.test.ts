import { afterEach, describe, expect, it, vi } from 'vitest';
import { createMockBackend, stopMockRuns } from './mock';
import type { MockOptions } from './mock';
import { MOCK_FREE_TENANT, MOCK_MANAGE_START_MS, MOCK_TEAM_SERVER, MOCK_TEAM_TENANT, manageScenario } from './mockManage';
import * as s from './schema';
import type { AppEvent, FreeServerPage, ManageDetail, ManagedServer, YourServersPage } from './schema';

afterEach(() => {
  vi.useRealTimers();
  stopMockRuns();
});

const backend = (manage?: MockOptions['manage'], over: MockOptions = {}) =>
  createMockBackend({ perland: 'signedIn', free: 'has', manage, ...over });
type Backend = ReturnType<typeof backend>;
const team = { tenantId: MOCK_TEAM_TENANT, stack: MOCK_TEAM_SERVER };
const servers = async (b: Backend) => s.managedServer.array().parse(await b.handle('managed_servers')) as ManagedServer[];
const detail = async (b: Backend) => s.manageDetail.parse(await b.handle('manage_detail', team)) as ManageDetail;
const refusal = async (p: Promise<unknown>) => {
  try {
    await p;
  } catch (e) {
    return e as { code: string; params: Record<string, string> };
  }
  throw new Error('expected a refusal');
};

describe('mock Manage', () => {
  it('reads the ?manage= param', () => {
    expect(manageScenario('crashed')).toBe('crashed');
    expect(manageScenario('nope')).toBeUndefined();
    expect(manageScenario(null)).toBeUndefined();
  });

  it('lists the free server first and the team server, both valid', async () => {
    const list = await servers(backend());
    expect(list.map((m) => [m.name, m.free, m.teamName])).toEqual([
      ['Weekend SMP', true, 'Perland'],
      [MOCK_TEAM_SERVER, false, 'Builders'],
    ]);
  });

  it('lists nothing while signed out or for `none`', async () => {
    expect(await servers(backend(undefined, { perland: 'signedOut' }))).toEqual([]);
    expect(await servers(backend('none'))).toEqual([]);
  });

  it('marks the free server card and adds the team server to Your servers', async () => {
    const b = backend();
    const page = s.freeServerPage.parse(await b.handle('free_server')) as FreeServerPage;
    expect(page.server?.manage).toEqual({ tenantId: MOCK_FREE_TENANT, stack: 'Weekend SMP' });
    const yours = s.yourServersPage.parse(await b.handle('your_servers')) as YourServersPage;
    expect(yours.managed.map((m) => m.name)).toEqual([MOCK_TEAM_SERVER]);

    const none = backend('none');
    expect((s.freeServerPage.parse(await none.handle('free_server')) as FreeServerPage).server?.manage).toBeNull();
    expect((s.yourServersPage.parse(await none.handle('your_servers')) as YourServersPage).managed).toEqual([]);
  });

  it.each([
    [undefined, 'running'],
    ['running', 'running'],
    ['asleep', 'asleep'],
    ['stopped', 'stopped'],
    ['crashed', 'stopped'],
    ['starting', 'starting'],
    ['locked', 'running'],
    ['modpack', 'running'],
    ['motdCodes', 'running'],
  ] as const)('%s: the detail reads %s', async (scenario, state) => {
    const d = await detail(backend(scenario));
    expect(d.state).toBe(state);
  });

  it('running: 3 playing', async () => {
    const d = await detail(backend());
    expect(d).toMatchObject({ online: 3, max: 10 });
    expect(d.players).toHaveLength(3);
  });

  it('starting: running after 3 s', async () => {
    vi.useFakeTimers();
    const b = backend('starting');
    expect((await detail(b)).state).toBe('starting');
    vi.advanceTimersByTime(MOCK_MANAGE_START_MS);
    expect((await detail(b)).state).toBe('running');
  });

  it('crashed: the diagnosis is out of memory', async () => {
    const d = s.manageDiagnosis.parse(await backend('crashed').handle('manage_diagnosis', team));
    expect(d).toMatchObject({ failed: true, oomKilled: true, cause: { code: 'out_of_memory', known: true } });
    const ok = s.manageDiagnosis.parse(await backend().handle('manage_diagnosis', team));
    expect(ok).toEqual({ failed: false, oomKilled: false, cause: null });
  });

  it('start, stop and restart move the state', async () => {
    vi.useFakeTimers();
    const b = backend('stopped');
    await b.handle('manage_action', { ...team, action: 'start' });
    expect((await detail(b)).state).toBe('starting');
    vi.advanceTimersByTime(MOCK_MANAGE_START_MS);
    expect((await detail(b)).state).toBe('running');
    await b.handle('manage_action', { ...team, action: 'stop' });
    expect((await detail(b)).state).toBe('stopped');
  });

  it('settings: modpack locks the version, motdCodes makes the MOTD read only', async () => {
    const plain = s.manageSettings.parse(await backend().handle('manage_settings', team));
    expect(plain).toMatchObject({ versionLocked: false, motdReadOnly: false });
    expect(s.manageSettings.parse(await backend('modpack').handle('manage_settings', team)).versionLocked).toBe(true);
    const codes = s.manageSettings.parse(await backend('motdCodes').handle('manage_settings', team));
    expect(codes.motdReadOnly).toBe(true);
    expect(codes.motd).toContain('§');
  });

  it('saves settings, refuses a stale revision, and locked refuses more players', async () => {
    const b = backend();
    const read = s.manageSettings.parse(await b.handle('manage_settings', team));
    const save = { ...read, motd: 'Hi', version: null, apply: 'next_restart' } as Record<string, unknown>;
    delete save.motdReadOnly;
    delete save.versionLocked;
    delete save.pendingRestart;
    const saved = s.manageSaved.parse(await b.handle('manage_settings_save', { ...team, save }));
    expect(saved).toMatchObject({ pending: true, restarted: false, settings: { motd: 'Hi', pendingRestart: true } });
    expect(saved.settings.revision).not.toBe(read.revision);
    expect((await refusal(b.handle('manage_settings_save', { ...team, save }))).code).toBe('settings_changed');

    const locked = backend('locked');
    const lockedRead = s.manageSettings.parse(await locked.handle('manage_settings', team));
    const more = { ...save, revision: lockedRead.revision, maxPlayers: 50 };
    const e = await refusal(locked.handle('manage_settings_save', { ...team, save: more }));
    expect(e.code).toBe('plan_feature_locked');
    expect(e.params.field).toBe('maxPlayers');
  });

  it('players: lists, and changes answer per name', async () => {
    const b = backend();
    const players = s.managePlayers.parse(await b.handle('manage_players', team));
    expect(players.online).toHaveLength(3);
    const results = s.manageChangeResult
      .array()
      .parse(await b.handle('manage_players_change', { ...team, change: { list: 'bans', add: true, names: ['Griefer2', players.bans[0]!.name] } }));
    expect(results.map((r) => r.status)).toEqual(['banned', 'already_banned']);
    expect(s.managePlayers.parse(await b.handle('manage_players', team)).bans.map((ban) => ban.name)).toContain('Griefer2');
  });

  it('console: echoes, asks confirmation for stop, then stops', async () => {
    const b = backend();
    const reply = s.manageConsoleReply.parse(await b.handle('manage_console', { ...team, command: 'say hi', confirm: false }));
    expect(reply.ok).toBe(true);
    expect(reply.output).toContain('say hi');
    expect((await refusal(b.handle('manage_console', { ...team, command: 'stop', confirm: false }))).code).toBe('command_needs_confirmation');
    expect((await detail(b)).state).toBe('running');
    await b.handle('manage_console', { ...team, command: 'stop', confirm: true });
    expect((await detail(b)).state).toBe('stopped');
  });

  it('console: a server_log line every 2 s while open, none after close', async () => {
    vi.useFakeTimers();
    const b = backend();
    const seen: AppEvent[] = [];
    b.state.manage.emit = (e: AppEvent) => seen.push(e);
    await b.handle('manage_console_open', team);
    vi.advanceTimersByTime(4000);
    expect(seen.filter((e) => e.type === 'server_log')).toHaveLength(2);
    for (const e of seen) expect(s.appEvent.safeParse(e).success).toBe(true);
    await b.handle('manage_console_close', team);
    vi.advanceTimersByTime(4000);
    expect(seen).toHaveLength(2);
  });

  it('opens the panel and answers null', async () => {
    const b = backend();
    expect(await b.handle('manage_open_panel', { ...team, stack: null })).toBeNull();
    expect(b.state.manage.opened).toEqual([{ tenantId: MOCK_TEAM_TENANT, stack: null }]);
  });

  it('refuses an unknown server', async () => {
    expect((await refusal(backend().handle('manage_detail', { tenantId: 'x', stack: 'y' }))).code).toBe('server_not_found');
  });
});
