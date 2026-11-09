import { afterEach, describe, expect, it, vi } from 'vitest';
import { createMockBackend, stopMockRuns } from './mock';
import { MOCK_LINK_CODE, MOCK_LINK_POLLS, perlandScenario } from './mockPerland';
import { MOCK_PUBLIC_SERVERS } from './mockServers';
import { inviteCanonical } from './inviteLink';
import type { MockOptions } from './mock';
import type { FreeServerPage, LinkCode, PerlandAccount, PerlandStats, PerlandStatus, YourServersPage } from './schema';
import * as s from './schema';

afterEach(() => {
  vi.useRealTimers();
  stopMockRuns();
});

const backend = (perland?: MockOptions['perland']) => createMockBackend({ perland });
const status = async (b: ReturnType<typeof backend>) => (await b.handle('perland_status')) as PerlandStatus;
const page = async (b: ReturnType<typeof backend>) => (await b.handle('your_servers')) as YourServersPage | null;

describe('mock Perland account', () => {
  it('is signed out and offers only the Perland page by default, panel or not', async () => {
    const st = await status(backend());
    expect(st).toEqual({ enabled: true, methods: ['perland'], account: null });
    expect(s.perlandStatus.safeParse(st).success).toBe(true);
    expect((await status(createMockBackend({ panel: false }))).methods).toEqual(['perland']);
  });

  it('signs in on the Perland page, and ends a sign-in as asked', async () => {
    vi.useFakeTimers();
    const ok = backend();
    const done = ok.handle('perland_sign_in', { language: 'en' });
    await vi.runAllTimersAsync();
    const account = (await done) as PerlandAccount;
    expect(account).toMatchObject({ name: 'Steve', newPlayer: true });
    expect(s.perlandAccount.safeParse(account).success).toBe(true);
    expect((await status(ok)).account).toEqual(account);
    for (const [outcome, code] of [
      ['cancel', 'sign_in_canceled'],
      ['timeout', 'sign_in_timed_out'],
      ['fail', 'sign_in_failed'],
      ['expired', 'sign_in_expired'],
    ] as const) {
      const b = createMockBackend({ perlandSignIn: outcome });
      const failed = b.handle('perland_sign_in', { language: 'en' });
      const caught = expect(failed).rejects.toMatchObject({ code });
      await vi.runAllTimersAsync();
      await caught;
    }
  });

  it('has no older sign-in commands', async () => {
    for (const cmd of ['perland_sign_in_discord', 'perland_sign_in_github', 'perland_email_start', 'perland_email_verify']) {
      await expect(backend().handle(cmd, { language: 'en', email: 'a@b.co', code: '123456' })).rejects.toMatchObject({ code: 'internal' });
    }
  });

  it('turns accounts off', async () => {
    const b = backend('off');
    expect(await status(b)).toEqual({ enabled: false, methods: [], account: null });
    await expect(b.handle('perland_sign_in', { language: 'en' })).rejects.toMatchObject({ code: 'player_accounts_closed' });
    await expect(b.handle('perland_account_email_start', { email: 'a@b.co', language: 'en' })).rejects.toMatchObject({
      code: 'player_accounts_closed',
    });
    expect(await page(b)).toBeNull();
  });

  it('resolves null when the player cancels the browser step', async () => {
    vi.useFakeTimers();
    const b = backend();
    const done = b.handle('perland_sign_in', { language: 'en' });
    await b.handle('perland_sign_in_cancel');
    expect(await done).toBeNull();
    expect((await status(b)).account).toBeNull();
  });

  it('adds an email only while signed in', async () => {
    await expect(backend().handle('perland_account_email_start', { email: 'a@b.co', language: 'en' })).rejects.toMatchObject({
      code: 'perland_signed_out',
    });
    await expect(backend().handle('perland_account_email_verify', { email: 'a@b.co', code: '123456' })).rejects.toMatchObject({
      code: 'perland_signed_out',
    });
  });

  it('counts wrong codes down from 4 to 0, then accepts 123456 and puts the email on the account', async () => {
    const b = backend('github');
    expect(await b.handle('perland_account_email_start', { email: 'alex@example.com', language: 'en' })).toEqual({ resendIn: 60 });
    for (const left of ['4', '3', '2', '1', '0']) {
      await expect(b.handle('perland_account_email_verify', { email: 'alex@example.com', code: '000000' })).rejects.toMatchObject({
        category: 'auth',
        code: 'invalid_code',
        params: { attemptsLeft: left },
      });
    }
    const account = (await b.handle('perland_account_email_verify', { email: 'alex@example.com', code: '123 456' })) as PerlandAccount;
    expect(account).toMatchObject({ name: 'octocat', email: 'alex@example.com', github: true });
    expect(s.perlandAccount.safeParse(account).success).toBe(true);
    expect((await status(b)).account).toEqual(account);
  });

  it('rejects a bad address and starts the count over for a new code', async () => {
    const b = backend('signedIn');
    await expect(b.handle('perland_account_email_start', { email: 'nope', language: 'en' })).rejects.toMatchObject({ code: 'invalid_email' });
    await b.handle('perland_account_email_start', { email: 'a@b.co', language: 'en' });
    await expect(b.handle('perland_account_email_verify', { email: 'a@b.co', code: '1' })).rejects.toMatchObject({ params: { attemptsLeft: '4' } });
    await b.handle('perland_account_email_start', { email: 'a@b.co', language: 'en' });
    await expect(b.handle('perland_account_email_verify', { email: 'a@b.co', code: '1' })).rejects.toMatchObject({ params: { attemptsLeft: '4' } });
  });

  it('lets a player missing an email create a free server once it is verified', async () => {
    const b = createMockBackend({ perland: 'github', free: 'emailRequired' });
    const create = async () => ((await b.handle('free_server')) as FreeServerPage).create;
    expect(await create()).toEqual({ allowed: false, reason: 'email_required', retryAfter: null });
    await b.handle('perland_account_email_start', { email: 'a@b.co', language: 'en' });
    await expect(b.handle('perland_account_email_verify', { email: 'a@b.co', code: '000000' })).rejects.toMatchObject({ code: 'invalid_code' });
    expect((await create()).allowed).toBe(false);
    await b.handle('perland_account_email_verify', { email: 'a@b.co', code: '123456' });
    expect(await create()).toEqual({ allowed: true, reason: null, retryAfter: null });
  });

  it('signs out', async () => {
    const b = backend('signedIn');
    expect((await status(b)).account).toMatchObject({ name: 'Steve' });
    await b.handle('perland_sign_out');
    expect((await status(b)).account).toBeNull();
    expect(await page(b)).toBeNull();
  });
});

describe('mock Your servers', () => {
  it('is null signed out and empty when there are none', async () => {
    expect(await page(backend())).toBeNull();
    expect(await page(backend('empty'))).toEqual({ servers: [], managed: [] });
    expect((await status(backend('empty'))).account).not.toBeNull();
  });

  it('serves one owner, one invited and two followed servers, in that order', async () => {
    const result = (await page(backend('signedIn'))) as YourServersPage;
    expect(s.yourServersPage.safeParse(result).success).toBe(true);
    expect(result.servers.map((c) => c.relation)).toEqual(['owner', 'invited', 'followed', 'followed']);
    expect(result.servers.find((c) => c.relation === 'owner')?.status).toBe('offline');
    expect(result.servers.find((c) => c.relation === 'invited')?.whitelisted).toBe(true);
  });

  it('shares one followed server with the public list, in the same canonical form', async () => {
    const result = (await page(backend('signedIn'))) as YourServersPage;
    const shared = result.servers.filter((c) => c.relation === 'followed' && MOCK_PUBLIC_SERVERS.some((p) => p.token === c.token));
    expect(shared).toHaveLength(1);
    const pub = MOCK_PUBLIC_SERVERS.find((p) => p.token === shared[0]!.token)!;
    expect(shared[0]!.invite).toBe(pub.invite);
    expect(inviteCanonical(shared[0]!.invite as string)).toBe(shared[0]!.invite);
  });

  it('follows a public server, then lists it as followed', async () => {
    const b = backend('signedIn');
    const target = MOCK_PUBLIC_SERVERS.find((p) => p.token.startsWith('B'))!;
    expect((await page(b))!.servers.some((c) => c.token === target.token)).toBe(false);
    await b.handle('your_servers_follow', { invite: target.invite });
    await b.handle('your_servers_follow', { invite: target.invite });
    const after = (await page(b))!.servers.filter((c) => c.token === target.token);
    expect(after).toEqual([expect.objectContaining({ relation: 'followed', following: true, name: target.name })]);
  });

  it('ignores follow when signed out or for an unknown invite', async () => {
    const out = backend();
    await out.handle('your_servers_follow', { invite: MOCK_PUBLIC_SERVERS[0]!.invite });
    const b = backend('empty');
    await b.handle('your_servers_follow', { invite: `https://play.example.com/${'Z'.repeat(43)}` });
    expect((await page(b))!.servers).toEqual([]);
  });

  it('unfollows a followed server, and leaves an owned one alone', async () => {
    const b = backend('signedIn');
    const before = (await page(b))!.servers;
    const followed = before.find((c) => c.relation === 'followed')!;
    const owner = before.find((c) => c.relation === 'owner')!;
    await b.handle('your_servers_unfollow', { invite: followed.invite });
    await b.handle('your_servers_unfollow', { invite: owner.invite });
    const after = (await page(b))!.servers;
    expect(after.map((c) => c.token)).toEqual(before.map((c) => c.token).filter((t) => t !== followed.token));
  });

  it('forgets an invited or followed server, and leaves an owned one alone', async () => {
    const b = backend('signedIn');
    const before = (await page(b))!.servers;
    const invited = before.find((c) => c.relation === 'invited')!;
    const followed = before.find((c) => c.relation === 'followed')!;
    const owner = before.find((c) => c.relation === 'owner')!;
    await b.handle('your_servers_forget', { invite: invited.invite });
    await b.handle('your_servers_forget', { invite: followed.invite });
    await b.handle('your_servers_forget', { invite: owner.invite });
    const after = (await page(b))!.servers;
    expect(after.map((c) => c.token)).toEqual(
      before.map((c) => c.token).filter((t) => t !== invited.token && t !== followed.token),
    );
  });

  it('records a joined server as invited, once', async () => {
    const b = backend('empty');
    const target = MOCK_PUBLIC_SERVERS.find((p) => p.whitelisted)!;
    await b.handle('your_servers_joined', { invite: target.invite });
    await b.handle('your_servers_joined', { invite: target.invite });
    expect((await page(b))!.servers.map((c) => [c.token, c.relation])).toEqual([[target.token, 'invited']]);
  });

  it('shows the signed-in set after signing in from signed out', async () => {
    vi.useFakeTimers();
    const b = backend();
    const done = b.handle('perland_sign_in', { language: 'en' });
    await vi.runAllTimersAsync();
    await done;
    expect((await page(b))!.servers.length).toBe(4);
  });
});

const stats = async (b: ReturnType<typeof backend>) => (await b.handle('perland_stats')) as PerlandStats | null;

describe('mock Minecraft link and stats', () => {
  it('accepts the linked and noAddon scenarios', () => {
    expect(perlandScenario('linked')).toBe('linked');
    expect(perlandScenario('noAddon')).toBe('noAddon');
    expect(perlandScenario('nope')).toBeUndefined();
  });

  it('is signed in and not linked, with no server stats, by default for signedIn', async () => {
    const b = backend('signedIn');
    expect((await status(b)).account).toMatchObject({ name: 'Steve', minecraft: null });
    expect(await stats(b)).toEqual({ servers: [] });
  });

  it('has no stats while signed out', async () => {
    expect(await stats(backend())).toBeNull();
  });

  it('refuses the link code and the poll while signed out, like the Engine', async () => {
    for (const b of [backend('signedOut'), backend('off')]) {
      const signedOut = { category: 'auth', code: 'perland_signed_out' };
      await expect(b.handle('perland_link_code')).rejects.toMatchObject(signedOut);
      await expect(b.handle('perland_link_poll')).rejects.toMatchObject(signedOut);
    }
  });

  it('links on the third poll, then shows the name on the account', async () => {
    const b = backend('signedIn');
    const code = (await b.handle('perland_link_code')) as LinkCode;
    expect(code).toEqual({ code: MOCK_LINK_CODE, expiresIn: 600 });
    expect(s.linkCode.safeParse(code).success).toBe(true);
    expect(MOCK_LINK_POLLS).toBe(3);
    expect(await b.handle('perland_link_poll')).toBeNull();
    expect(await b.handle('perland_link_poll')).toBeNull();
    expect((await status(b)).account?.minecraft).toBeNull();
    expect(await b.handle('perland_link_poll')).toBe('Steve');
    expect((await status(b)).account?.minecraft).toBe('Steve');
  });

  it('starts the count over for a new code', async () => {
    const b = backend('signedIn');
    await b.handle('perland_link_code');
    await b.handle('perland_link_poll');
    await b.handle('perland_link_code');
    expect(await b.handle('perland_link_poll')).toBeNull();
    expect(await b.handle('perland_link_poll')).toBeNull();
    expect(await b.handle('perland_link_poll')).toBe('Steve');
  });

  it('unlinks', async () => {
    const b = backend('linked');
    expect((await status(b)).account?.minecraft).toBe('Steve');
    expect(await b.handle('perland_unlink')).toBeNull();
    expect((await status(b)).account?.minecraft).toBeNull();
  });

  it('answers null stats when the add-on is off', async () => {
    const b = backend('noAddon');
    expect((await status(b)).account).toMatchObject({ name: 'Steve', minecraft: null });
    expect(await stats(b)).toBeNull();
  });

  it('serves stats for the owner, the shared followed server and the server instance', async () => {
    const b = createMockBackend({ perland: 'linked', server: 'online' });
    const result = (await stats(b)) as PerlandStats;
    expect(s.perlandStats.safeParse(result).success).toBe(true);
    expect(result.servers.map((x) => [x.token[0], x.advancements, x.playtimeSeconds])).toEqual([
      ['O', 12, 108000],
      ['F', 3, 2400],
      ['A', 7, expect.any(Number)],
    ]);
    expect(result.servers.find((x) => x.token.startsWith('A'))?.instance).toBe('survival');
    expect(result.servers.find((x) => x.token.startsWith('O'))?.instance).toBeNull();
  });

  it('leaves the instance null when the server instance was never added', async () => {
    const result = (await stats(backend('linked'))) as PerlandStats;
    expect(result.servers.find((x) => x.token.startsWith('A'))?.instance).toBeNull();
  });

  it('shows the linked stats after linking from signedIn', async () => {
    const b = backend('signedIn');
    for (let i = 0; i < MOCK_LINK_POLLS; i++) await b.handle('perland_link_poll');
    expect(((await stats(b)) as PerlandStats).servers.length).toBeGreaterThan(0);
  });
});
