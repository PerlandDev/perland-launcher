import { afterEach, describe, expect, it, vi } from 'vitest';
import { createMockBackend, stopMockRuns } from './mock';
import type { MockOptions } from './mock';
import { freeScenario, MOCK_FREE_CREATE_MS, MOCK_FREE_INVITE } from './mockFree';
import { inviteCanonical } from './inviteLink';
import * as s from './schema';
import type { FreeServerCard, FreeServerPage, Instance, YourServersPage } from './schema';

afterEach(() => {
  vi.useRealTimers();
  stopMockRuns();
});

const backend = (free?: MockOptions['free'], perland: MockOptions['perland'] = 'signedIn') => createMockBackend({ perland, free });
type Backend = ReturnType<typeof backend>;
const page = async (b: Backend) => (await b.handle('free_server')) as FreeServerPage | null;
const create = (b: Backend, over: Record<string, unknown> = {}) =>
  b.handle('free_server_create', { name: 'Weekend', kind: 'vanilla', modrinthVersionId: null, language: 'en', ...over });

describe('mock free server page', () => {
  it('is allowed and empty by default, and validates against the schema', async () => {
    const p = await page(backend());
    expect(p).toMatchObject({ create: { allowed: true, reason: null, retryAfter: null }, server: null });
    expect(p?.limits).toMatchObject({ maxPlayers: 10, memoryMb: { vanilla: 2048, modded: 6144 }, createsPerWeek: 3 });
    expect(s.freeServerPage.safeParse(p).success).toBe(true);
  });

  it('only answers while the player is signed in', async () => {
    expect(await page(backend('has', 'signedOut'))).toBeNull();
    expect(await page(backend('has', 'off'))).toBeNull();
    expect(await page(backend('has', 'empty'))).not.toBeNull();
  });

  it('is off for `off`', async () => {
    expect(await page(backend('off'))).toBeNull();
  });

  it('has: a sleeping server with an invite and a deletion that is not warned yet', async () => {
    const p = (await page(backend('has'))) as FreeServerPage;
    expect(s.freeServerPage.safeParse(p).success).toBe(true);
    expect(p.create).toMatchObject({ allowed: false, reason: 'free_server_exists' });
    const card = p.server as FreeServerCard;
    expect(card).toMatchObject({ status: 'sleeping', freeStatus: 'sleeping', joinable: true, kind: 'vanilla', deletion: { warned: false } });
    expect(card.invite).toBe(MOCK_FREE_INVITE);
    expect(card.inviteUrl).toBe(MOCK_FREE_INVITE);
    expect(card.joinUrl).toBe(`perland://join?invite=${encodeURIComponent(MOCK_FREE_INVITE)}`);
    expect(card.disk.over).toBe(false);
  });

  it('warned: the deletion is warned', async () => {
    const card = (await page(backend('warned')))?.server;
    expect(card?.deletion?.warned).toBe(true);
  });

  it('overDisk: stopped, over its limit and not joinable; the card folds stopped into offline', async () => {
    const p = (await page(backend('overDisk'))) as FreeServerPage;
    expect(s.freeServerPage.safeParse(p).success).toBe(true);
    expect(p.server).toMatchObject({ freeStatus: 'stopped', status: 'offline', joinable: false, disk: { over: true } });
    expect(p.server!.disk.usedBytes).toBeGreaterThan(p.server!.disk.limitBytes);
  });

  it('creating: the card folds creating into sleeping and has no invite yet', async () => {
    const p = (await page(backend('creating'))) as FreeServerPage;
    expect(s.freeServerPage.safeParse(p).success).toBe(true);
    expect(p.server).toMatchObject({ freeStatus: 'creating', status: 'sleeping', joinable: false, invite: null, inviteUrl: null, joinUrl: null });
  });

  it.each([
    ['emailRequired', { allowed: false, reason: 'email_required', retryAfter: null }],
    ['rateLimited', { allowed: false, reason: 'too_many_free_servers', retryAfter: 7200 }],
    ['full', { allowed: false, reason: 'free_servers_full', retryAfter: null }],
    ['exists', { allowed: false, reason: 'free_server_exists', retryAfter: null }],
  ] as const)('%s: create is refused with its reason and there is no server', async (scenario, create) => {
    const p = (await page(backend(scenario))) as FreeServerPage;
    expect(s.freeServerPage.safeParse(p).success).toBe(true);
    expect(p).toMatchObject({ create, server: null });
  });
});

describe('mock free server create', () => {
  it('takes 2 s and then answers starting; the page then holds it', async () => {
    vi.useFakeTimers();
    const b = backend();
    let done: FreeServerCard | null = null;
    const pending = create(b, { name: 'Weekend' }).then((c) => (done = c as FreeServerCard));
    await vi.advanceTimersByTimeAsync(MOCK_FREE_CREATE_MS - 1);
    expect(done).toBeNull();
    await vi.advanceTimersByTimeAsync(1);
    await pending;
    expect(s.freeServerCard.safeParse(done).success).toBe(true);
    expect(done).toMatchObject({ name: 'Weekend', status: 'starting', freeStatus: 'starting', kind: 'vanilla', memoryMb: 2048, maxPlayers: 10 });
    const p = (await page(b)) as FreeServerPage;
    expect(p.server?.name).toBe('Weekend');
    expect(p.create).toMatchObject({ allowed: false, reason: 'free_server_exists' });
  });

  it('makes a modded server of 6144 MB from a Modrinth version', async () => {
    vi.useFakeTimers();
    const b = backend();
    const pending = create(b, { name: 'Cozy', kind: 'modrinth', modrinthVersionId: 'kL2pYz4w' }) as Promise<FreeServerCard>;
    await vi.advanceTimersByTimeAsync(MOCK_FREE_CREATE_MS);
    expect(await pending).toMatchObject({ kind: 'modrinth', memoryMb: 6144, modded: true });
  });

  it('answers modpack_too_heavy at once for a modpack whose project ends in heavy', async () => {
    const b = backend();
    const heavy = (b.state.listing.instances.find((i) => i.source.kind === 'modrinth' && i.source.projectId.endsWith('heavy')) as Instance).source;
    if (heavy.kind !== 'modrinth') throw new Error('unreachable');
    await expect(create(b, { kind: 'modrinth', modrinthVersionId: heavy.versionId })).rejects.toMatchObject({
      category: 'pack',
      code: 'modpack_too_heavy',
      params: { requiredMemoryMb: '8192', limitMemoryMb: '6144' },
    });
    expect((await page(b))?.server).toBeNull();
  });

  it.each([
    ['emailRequired', 'email_required'],
    ['rateLimited', 'too_many_free_servers'],
    ['full', 'free_servers_full'],
    ['exists', 'free_server_exists'],
    ['has', 'free_server_exists'],
  ] as const)('%s refuses with %s', async (scenario, code) => {
    await expect(create(backend(scenario))).rejects.toMatchObject({ code });
  });

  it('refuses a bad name or kind', async () => {
    const b = backend();
    await expect(create(b, { name: 'no spaces' })).rejects.toMatchObject({ code: 'invalid_free_server' });
    await expect(create(b, { kind: 'bedrock' })).rejects.toMatchObject({ code: 'invalid_free_server' });
    await expect(create(b, { kind: 'modrinth', modrinthVersionId: null })).rejects.toMatchObject({ code: 'invalid_free_server' });
  });

  it('refuses while signed out', async () => {
    await expect(create(backend('none', 'signedOut'))).rejects.toMatchObject({ code: 'perland_signed_out' });
  });
});

describe('mock free server start, delete and play', () => {
  it('deleting clears the server and allows creating again', async () => {
    const b = backend('has');
    expect(await b.handle('free_server_delete')).toBe(true);
    expect(await page(b)).toMatchObject({ create: { allowed: true }, server: null });
  });

  it('starting wakes a sleeping server, and refuses one that is out of space', async () => {
    const b = backend('has');
    expect(await b.handle('free_server_start')).toMatchObject({ freeStatus: 'starting', status: 'starting' });
    expect((await page(b))?.server?.freeStatus).toBe('starting');
    await expect(backend('overDisk').handle('free_server_start')).rejects.toMatchObject({ code: 'free_server_over_disk' });
  });

  it('play joins through the invite and answers the instance id; the server is then linked to it', async () => {
    const b = backend('has');
    const id = (await b.handle('free_server_play')) as string;
    const instance = b.state.listing.instances.find((i) => i.id === id) as Instance;
    expect(instance.source.kind).toBe('perland');
    expect(instance.source.kind === 'perland' && inviteCanonical(instance.source.invite)).toBe(inviteCanonical(MOCK_FREE_INVITE));
    expect((await page(b))?.server?.instance).toBe(id);
    expect(await b.handle('free_server_play')).toBe(id);
  });

  it('play refuses creating and out of space, and starts an offline server', async () => {
    await expect(backend('creating').handle('free_server_play')).rejects.toMatchObject({ code: 'invite_unsupported' });
    await expect(backend('overDisk').handle('free_server_play')).rejects.toMatchObject({ code: 'free_server_over_disk' });
  });

  it('the free server joins through the mock join flow', async () => {
    const b = backend('has');
    const preview = (await b.handle('join_preview', { link: MOCK_FREE_INVITE })) as { name: string };
    expect(preview.name).toBe('Weekend SMP');
  });
});

describe('mock Your servers next to the free server', () => {
  const servers = async (b: Backend) => ((await b.handle('your_servers')) as YourServersPage).servers;

  it.each(['has', 'warned', 'overDisk', 'creating'] as const)('%s: no owner entry duplicates the free server', async (scenario) => {
    const b = backend(scenario);
    const free = (await page(b))!.server!;
    const list = await servers(b);
    const duplicates = list.filter(
      (c) => c.relation === 'owner' && ((free.invite && c.invite && inviteCanonical(c.invite) === inviteCanonical(free.invite)) || c.name === free.name),
    );
    expect(duplicates).toEqual([]);
  });

  it('drops an owner entry the free server would repeat, by invite or (with no invite) by name', async () => {
    const b = backend('has');
    b.state.perland.servers.push({ ...structuredClone(b.state.perland.servers[0]!), name: 'Copy', invite: MOCK_FREE_INVITE, relation: 'owner' });
    expect((await servers(b)).some((c) => c.name === 'Copy')).toBe(false);
    const c = backend('creating');
    c.state.perland.servers.push({ ...structuredClone(c.state.perland.servers[0]!), name: 'Weekend SMP', invite: null, relation: 'owner' });
    expect((await servers(c)).some((x) => x.name === 'Weekend SMP')).toBe(false);
  });
});

describe('mock instances for the free server', () => {
  const modrinth = (b: Backend) => b.state.listing.instances.filter((i) => i.source.kind === 'modrinth');

  it('has a Cozy Pack and a heavy Mega Pack while the free mock is active', () => {
    const b = backend();
    const cozy = modrinth(b).find((i) => i.name === 'Cozy Pack');
    expect(cozy?.source).toEqual({ kind: 'modrinth', projectId: 'aOfL7fd2', versionId: 'kL2pYz4w' });
    const mega = modrinth(b).find((i) => i.name === 'Mega Pack');
    expect(mega?.source).toMatchObject({ kind: 'modrinth', projectId: 'megaheavy' });
    for (const i of modrinth(b)) expect(s.instance.safeParse(i).success).toBe(true);
  });

  it('leaves the instances alone while signed out or off', () => {
    expect(modrinth(backend(undefined, 'signedOut'))).toEqual([]);
    expect(modrinth(backend('off'))).toEqual([]);
    expect(modrinth(createMockBackend({}))).toEqual([]);
  });
});

describe('?free= preview param', () => {
  it('knows the scenarios and ignores anything else', () => {
    for (const name of ['off', 'none', 'has', 'warned', 'overDisk', 'emailRequired', 'rateLimited', 'full', 'creating', 'exists']) expect(freeScenario(name)).toBe(name);
    expect(freeScenario('nope')).toBeUndefined();
    expect(freeScenario(null)).toBeUndefined();
  });
});
