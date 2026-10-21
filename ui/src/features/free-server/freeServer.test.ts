import { afterEach, describe, expect, it } from 'vitest';
import en from '../../i18n/en.json';
import es from '../../i18n/es.json';
import i18n from '../../i18n';
import { MOCK_FREE_LIMITS } from '../../ipc/mockFree';
import type { Instance, McVersion } from '../../ipc/schema';
import { friendsPath, isOldVersion, isServerName, limitsText, maybeCreated, prefillName, reasonLine, retryText } from './freeServer';

const t = i18n.t.bind(i18n);
afterEach(() => i18n.changeLanguage('en'));

/** Every key path under `value`, as `a.b.c`. */
const keysOf = (value: unknown, prefix = ''): string[] =>
  typeof value === 'object' && value !== null
    ? Object.entries(value).flatMap(([k, v]) => keysOf(v, prefix ? `${prefix}.${k}` : k))
    : [prefix];

const error = (code: string, params: Record<string, string> = {}) => ({ category: 'network', code, message: 'x', fix: null, detail: null, params });

describe('freeServer', () => {
  it('maybe_created_after_a_lost_answer_only', () => {
    // Core turns a proxy's 502 or 504 on the long create into `connection_dropped`: the Engine may still finish it.
    expect(maybeCreated(error('connection_dropped', { host: 'api.example.com', status: '504' }))).toBe(true);
    expect(maybeCreated(error('offline', { host: 'api.example.com' }))).toBe(true);
    expect(maybeCreated(error('free_server_exists'))).toBe(true);
    expect(maybeCreated(error('too_many_free_servers', { retryAfter: '5' }))).toBe(true);
    expect(maybeCreated(error('too_many_free_servers', { retryAfter: '600' }))).toBe(false);
    // Modrinth failing is the Engine's real refusal, and so is any other trouble with an answer.
    expect(maybeCreated(error('host_trouble', { host: 'modrinth.com', status: '502' }))).toBe(false);
    expect(maybeCreated(error('host_trouble', { host: 'api.example.com', status: '500' }))).toBe(false);
  });

  it('prefill_never_uses_the_email', () => {
    // An email-only player has no display name: the email never becomes the name.
    expect(prefillName('')).toBe('my-server');
    expect(prefillName('   ')).toBe('my-server');
    // A name with nothing left once cleaned is no name either.
    expect(prefillName('★★★')).toBe('my-server');
  });

  it('prefill_sanitizes', () => {
    expect(prefillName('Steve')).toBe('Steve');
    expect(prefillName('Cool Kid 99')).toBe('Cool-Kid-99');
    expect(prefillName('José María')).toBe('Jose-Maria');
    expect(prefillName('a.b!c_d-e')).toBe('abc_d-e');
    expect(prefillName('x'.repeat(40))).toBe('x'.repeat(32));
    for (const raw of ['Steve', 'Cool Kid 99', 'José María', '★ Moi ★', 'x'.repeat(40)]) expect(isServerName(prefillName(raw))).toBe(true);
    expect(isServerName('')).toBe(false);
    expect(isServerName('has space')).toBe(false);
    expect(isServerName('y'.repeat(33))).toBe(false);
  });

  it('retry_text_running_and_days', async () => {
    expect(retryText(t, 0)).toBe('A server is being created right now.');
    expect(retryText(t, 5)).toBe('A server is being created right now.');
    expect(retryText(t, 6)).toBe('You can create another server in 1 minute.');
    expect(retryText(t, 3000)).toBe('You can create another server in 50 minutes.');
    expect(retryText(t, 3599)).toBe('You can create another server in 1 hour.');
    expect(retryText(t, 7200)).toBe('You can create another server in 2 hours.');
    expect(retryText(t, 86_400 + 1)).toBe('You can create another server in 2 days.');
    expect(retryText(t, 2 * 86_400)).toBe('You can create another server in 2 days.');
    await i18n.changeLanguage('es');
    expect(retryText(t, 3)).toBe('Ya se está creando un servidor.');
    expect(retryText(t, 2 * 86_400)).toBe('Podrás crear otro servidor en 2 días.');
  });

  it('reason_lines', () => {
    const blocked = (reason: string, retryAfter: number | null = null) => ({
      allowed: false,
      reason,
      retryAfter,
    });
    expect(reasonLine(t, { allowed: true, reason: null, retryAfter: null }, false)).toBeNull();
    expect(reasonLine(t, blocked('email_required'), false)).toBe(
      'Free servers need a verified email on your Perland account. Add one, then try again.',
    );
    expect(reasonLine(t, blocked('free_server_exists'), false)).toBe('Another account with your email already has a free server.');
    // The player's own server shows as its card: no line.
    expect(reasonLine(t, blocked('free_server_exists'), true)).toBeNull();
    expect(reasonLine(t, blocked('too_many_free_servers', 3), false)).toBe('A server is being created right now.');
    expect(reasonLine(t, blocked('too_many_free_servers', 7200), false)).toBe('You can create another server in 2 hours.');
    expect(reasonLine(t, blocked('free_servers_full'), false)).toBe('Free servers are full right now. Try again later.');
    expect(reasonLine(t, blocked('host_full'), false)).toBe('Perland is full right now. Try again in a few minutes.');
    expect(reasonLine(t, blocked('free_servers_paused'), false)).toBe('New free servers are paused. Try again later.');
    expect(reasonLine(t, blocked('free_server_blocked'), false)).toBe("You can't create a free server.");
    expect(reasonLine(t, blocked('something_new'), false)).toBe("You can't create a server right now.");
  });

  it('limits_text_from_limits', () => {
    expect(limitsText(t, MOCK_FREE_LIMITS)).toBe(
      'Up to 10 players. 2 GB of memory for vanilla, 6 GB with mods. Sleeps when nobody plays and wakes when someone joins. Deleted after 30 days without players.',
    );
    // GB round down; numbers all come from the limits.
    const other = {
      ...MOCK_FREE_LIMITS,
      maxPlayers: 4,
      memoryMb: { vanilla: 3500, modded: 6100 },
      unusedDays: 14,
    };
    expect(limitsText(t, other)).toBe(
      'Up to 4 players. 3 GB of memory for vanilla, 5 GB with mods. Sleeps when nobody plays and wakes when someone joins. Deleted after 14 days without players.',
    );
    // Servers that never sleep are never deleted for being unused either.
    expect(limitsText(t, { ...MOCK_FREE_LIMITS, sleepMinutes: 0 })).toBe('Up to 10 players. 2 GB of memory for vanilla, 6 GB with mods.');
  });

  it('spanish_strings_exist', async () => {
    const enKeys = keysOf(en.freeServer).sort();
    expect(enKeys.length).toBeGreaterThan(10);
    expect(keysOf(es.freeServer).sort()).toEqual(enKeys);
    await i18n.changeLanguage('es');
    expect(limitsText(t, MOCK_FREE_LIMITS)).toBe(
      'Hasta 10 jugadores. 2 GB de memoria para vanilla, 6 GB con mods. Se duerme cuando nadie juega y despierta cuando alguien entra. Se borra tras 30 días sin jugadores.',
    );
  });
});

describe('friendsPath and isOldVersion', () => {
  const instance = (loader: Instance['game']['loader'], source: Instance['source']) => ({ game: { minecraft: '1.20.1', loader }, source }) as Instance;
  const fabric = { kind: 'fabric', version: '0.16.14' } as const;

  it('follows the app: no loader is vanilla first, then a Modrinth modpack, else the pack upload', () => {
    expect(friendsPath(instance(null, { kind: 'none' }))).toBe('vanilla');
    expect(friendsPath(instance(null, { kind: 'modrinth', projectId: 'p', versionId: 'v' }))).toBe('vanilla');
    expect(friendsPath(instance(fabric, { kind: 'modrinth', projectId: 'p', versionId: 'v' }))).toBe('modrinth');
    expect(friendsPath(instance(fabric, { kind: 'none' }))).toBe('pack');
    expect(friendsPath(instance(fabric, { kind: 'packwiz', url: 'https://x' }))).toBe('pack');
  });

  it('knows old alphas and betas from the list, or by their ids before it loads', () => {
    const list: McVersion[] = [
      { id: '26.3', kind: 'release', releaseTime: '' },
      { id: 'b1.7.3', kind: 'old_beta', releaseTime: '' },
      { id: 'c0.30_01c', kind: 'old_alpha', releaseTime: '' },
    ] as McVersion[];
    expect(isOldVersion('26.3', list)).toBe(false);
    expect(isOldVersion('b1.7.3', list)).toBe(true);
    expect(isOldVersion('c0.30_01c', list)).toBe(true);
    for (const id of ['b1.7.3', 'a1.2.6', 'rd-132211', 'inf-20100618']) expect(isOldVersion(id, undefined)).toBe(true);
    for (const id of ['1.21.1', '26.4-snapshot-3', '24w14a']) expect(isOldVersion(id, undefined)).toBe(false);
  });

  it('counts an id the Engine would refuse as not runnable, listed or not', () => {
    const list = [{ id: '1.14 Pre-Release 1', kind: 'snapshot', releaseTime: '' }] as McVersion[];
    expect(isOldVersion('1.14 Pre-Release 1', list)).toBe(true);
    expect(isOldVersion('1.14 Pre-Release 1', undefined)).toBe(true);
    expect(isOldVersion('x'.repeat(33), undefined)).toBe(true);
    expect(isOldVersion('', undefined)).toBe(true);
    expect(isOldVersion('1.21.1_custom-2', undefined)).toBe(false);
    expect(isOldVersion('x'.repeat(32), undefined)).toBe(false);
  });
});
