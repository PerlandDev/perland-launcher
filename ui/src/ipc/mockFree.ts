// Mock free server for `pnpm --filter ui dev:mock` and the tests: the page, create (2 s), start, delete and play.
// Only answers while the Perland mock is signed in. Fake tokens only.
import { inviteCanonical } from './inviteLink';
import { HOST, inviteOf, joinedInstance } from './mockServers';
import type {
  CreateAllowance,
  FreeLimits,
  FreeServerCard,
  FreeServerPage,
  Instance,
  LauncherError,
  ManageRef,
  PerlandAccount,
  PublicServerCard,
  YourServerCard,
} from './schema';

/** `?free=`: `off` (the Engine has free servers off), `none` (the default, may create), `has` (a sleeping server), `warned`
 *  (it will be deleted soon, and the player was told), `overDisk` (stopped, out of space), `emailRequired`, `rateLimited`,
 *  `full`, `hostFull` (Perland's machine is full), `paused` (new free servers are paused), `blocked` (this player can't create
 *  one), `creating` (being created, no invite yet) and `exists` (another account with the player's email has one). */
export type FreeScenario =
  | 'off' | 'none' | 'has' | 'warned' | 'overDisk' | 'emailRequired' | 'rateLimited' | 'full' | 'hostFull' | 'paused' | 'blocked'
  | 'creating' | 'exists';

const SCENARIOS: readonly string[] = [
  'off', 'none', 'has', 'warned', 'overDisk', 'emailRequired', 'rateLimited', 'full', 'hostFull', 'paused', 'blocked', 'creating', 'exists',
];
/** The `?free=` preview param, or undefined for anything else (dev:mock only). */
export const freeScenario = (name: string | null): FreeScenario | undefined =>
  name !== null && SCENARIOS.includes(name) ? (name as FreeScenario) : undefined;

/** How long the mock create takes. */
export const MOCK_FREE_CREATE_MS = 2000;
/** The invite of the mock free server (a card of the mock join flow once the server exists). */
export const MOCK_FREE_INVITE = inviteOf('Z');
const LETTER = 'Z';
const DAY = 86_400_000;
const GIB = 1024 ** 3;

export const MOCK_FREE_LIMITS: FreeLimits = {
  maxPlayers: 10,
  memoryMb: { vanilla: 2048, modded: 6144 },
  diskBytes: 10 * GIB,
  sleepMinutes: 15,
  unusedDays: 30,
  warningDays: 7,
  createsPerWeek: 3,
  pack: { maxBytes: 64 * 1024 ** 2 },
};
/** What a modpack that needs more than a free server's memory asks for. */
const HEAVY_MEMORY_MB = 8192;

/** The Modrinth modpack instances the create dialog offers: one that fits and one whose project id ends in `heavy`. */
export const MOCK_COZY_PACK = { projectId: 'aOfL7fd2', versionId: 'kL2pYz4w' };
export const MOCK_MEGA_PACK = { projectId: 'megaheavy', versionId: 'mEgAhEaVy01' };

const modpack = (id: string, name: string, source: { projectId: string; versionId: string }): Instance => ({
  id,
  name,
  source: { kind: 'modrinth', ...source },
  game: { minecraft: '1.20.1', loader: { kind: 'fabric', version: '0.16.14' } },
  updatePolicy: 'ask',
  optionalChoices: {},
  java: { path: null, memoryMb: 6144, args: '' },
  server: null,
  artSeed: null,
  art: 'auto',
  icon: null,
  bannerFocus: null,
});
export const modrinthInstances = (): Instance[] => [modpack('cozy-pack', 'Cozy Pack', MOCK_COZY_PACK), modpack('mega-pack', 'Mega Pack', MOCK_MEGA_PACK)];

interface MockFree {
  scenario: FreeScenario;
  /** The player's free server, as the Engine holds it. */
  server: FreeServerCard | null;
}

interface Listing {
  instances: Instance[];
}

export function fail(category: LauncherError['category'], code: string, message: string, params: Record<string, string> = {}): never {
  throw { category, code, message, fix: null, detail: null, params } satisfies LauncherError;
}

const inDays = (days: number) => new Date(Date.now() + days * DAY).toISOString();

interface View {
  name: string;
  kind: 'vanilla' | 'modrinth';
  freeStatus: string;
  invited: boolean;
  deletion: FreeServerCard['deletion'];
  usedBytes: number;
}

/** The card the app builds from the Engine's free-server view (`status` folds creating/deleting into sleeping, stopped into offline). */
function cardOf(view: View): FreeServerCard {
  const modded = view.kind === 'modrinth';
  const over = view.usedBytes > MOCK_FREE_LIMITS.diskBytes;
  const status = view.freeStatus === 'stopped' ? 'offline' : ['creating', 'deleting'].includes(view.freeStatus) ? 'sleeping' : view.freeStatus;
  const unplayable = ['creating', 'deleting', 'stopped'].includes(view.freeStatus);
  const invite = view.invited ? MOCK_FREE_INVITE : null;
  return {
    token: view.invited ? LETTER.repeat(43) : '',
    invite,
    joinable: invite !== null && !unplayable && !over,
    address: view.invited ? `${HOST}:25590` : null,
    name: view.name,
    displayName: view.name,
    iconVersion: null,
    // The Engine's view sends `game {type, version}` only: the app takes a modpack's loader from `type`, and a vanilla
    // create (no version sent) runs and answers `LATEST`.
    game: { version: modded ? '1.20.1' : 'LATEST', loader: modded ? 'fabric' : null, loaderVersion: null },
    modded,
    status: status as PublicServerCard['status'],
    players: view.freeStatus === 'online' ? { online: 0, max: MOCK_FREE_LIMITS.maxPlayers } : null,
    whitelisted: false,
    pack: null,
    categories: [],
    language: '',
    region: null,
    instance: null,
    kind: view.kind,
    memoryMb: modded ? MOCK_FREE_LIMITS.memoryMb.modded : MOCK_FREE_LIMITS.memoryMb.vanilla,
    maxPlayers: MOCK_FREE_LIMITS.maxPlayers,
    freeStatus: view.freeStatus,
    deletion: view.deletion,
    disk: { usedBytes: view.usedBytes, limitBytes: MOCK_FREE_LIMITS.diskBytes, checkedAt: new Date().toISOString(), over },
    inviteUrl: invite,
    joinUrl: invite === null ? null : `perland://join?invite=${encodeURIComponent(invite)}`,
    // Set as it is answered: the Manage mock decides it.
    manage: null,
  };
}

export function seedFree(scenario: FreeScenario = 'none'): MockFree {
  const base: View = { name: 'Weekend SMP', kind: 'vanilla', freeStatus: 'sleeping', invited: true, deletion: { deletesAt: inDays(30), warned: false }, usedBytes: GIB };
  const views: Partial<Record<FreeScenario, View>> = {
    has: base,
    warned: { ...base, deletion: { deletesAt: inDays(5), warned: true } },
    overDisk: { ...base, freeStatus: 'stopped', deletion: null, usedBytes: 11 * GIB },
    creating: { ...base, freeStatus: 'creating', invited: false, deletion: null, usedBytes: 0 },
  };
  const view = views[scenario];
  return { scenario, server: view ? cardOf(view) : null };
}

const BLOCKED: Partial<Record<FreeScenario, CreateAllowance>> = {
  emailRequired: { allowed: false, reason: 'email_required', retryAfter: null },
  rateLimited: { allowed: false, reason: 'too_many_free_servers', retryAfter: 7200 },
  full: { allowed: false, reason: 'free_servers_full', retryAfter: null },
  hostFull: { allowed: false, reason: 'host_full', retryAfter: null },
  paused: { allowed: false, reason: 'free_servers_paused', retryAfter: null },
  blocked: { allowed: false, reason: 'free_server_blocked', retryAfter: null },
  exists: { allowed: false, reason: 'free_server_exists', retryAfter: null },
};

const allowance = (free: MockFree): CreateAllowance =>
  free.server ? { allowed: false, reason: 'free_server_exists', retryAfter: null } : (BLOCKED[free.scenario] ?? { allowed: true, reason: null, retryAfter: null });

/** Whether `card` (a Your servers entry) repeats the free server: the same invite, or the same name while the free server has no invite. */
export function repeatsFreeServer(free: MockFree, card: YourServerCard): boolean {
  const own = free.server;
  if (!own || card.relation !== 'owner') return false;
  if (own.invite !== null) return card.invite !== null && inviteCanonical(card.invite) === inviteCanonical(own.invite);
  return card.name === own.name;
}

const refuse = (free: MockFree): never => {
  const a = allowance(free);
  switch (a.reason) {
    case 'email_required':
      return fail('auth', 'email_required', 'Free servers need a verified email on your account.');
    case 'too_many_free_servers':
      return fail('network', 'too_many_free_servers', 'You have created too many free servers lately.', { retryAfter: String(a.retryAfter ?? 0) });
    case 'free_servers_full':
      return fail('network', 'free_servers_full', 'Free servers are full right now.');
    case 'host_full':
      return fail('network', 'host_full', 'Perland is full right now. Try again in a few minutes.');
    case 'free_servers_paused':
      return fail('network', 'free_servers_paused', 'New free servers are paused. Try again later.');
    case 'free_server_blocked':
      return fail('network', 'free_server_blocked', "You can't create a free server.");
    default:
      return fail('network', 'free_server_exists', 'You already have a free server.');
  }
};

const signedOut = (): never => fail('auth', 'perland_signed_out', 'Sign in to Perland again.');

const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

export const NOT_FREE = Symbol('not a free-server command');

/** Answer a free-server command, or `NOT_FREE`. `join` joins an invite as `join_add` does and answers the instance. */
export async function handleFree(
  free: MockFree,
  account: PerlandAccount | null,
  listing: Listing,
  cmd: string,
  args: Record<string, unknown>,
  join: (invite: string) => Instance,
  /** The Manage reference the card carries (the Manage mock's), or null. */
  manageOf: (card: FreeServerCard) => ManageRef | null = () => null,
): Promise<unknown> {
  if (!cmd.startsWith('free_server')) return NOT_FREE;
  const answer = (card: FreeServerCard): FreeServerCard => ({ ...structuredClone(card), manage: manageOf(card) });
  const on = free.scenario !== 'off';
  // The page is null while signed out or off; the others need an account.
  if (cmd === 'free_server') {
    if (!on || !account) return null;
    const server = free.server && { ...answer(free.server), instance: joinedInstance(free.server, listing) };
    return { limits: MOCK_FREE_LIMITS, create: allowance(free), server } satisfies FreeServerPage;
  }
  if (!account) signedOut();
  if (!on) fail('network', 'player_accounts_closed', "Free servers aren't open yet.");
  switch (cmd) {
    case 'free_server_create': {
      const name = String(args.name ?? '');
      const kind = args.kind;
      const version = typeof args.modrinthVersionId === 'string' ? args.modrinthVersionId : null;
      const valid =
        /^[A-Za-z0-9_-]{1,32}$/.test(name) &&
        (kind === 'vanilla' ? version === null : kind === 'modrinth' && version !== null && /^[A-Za-z0-9]{1,16}$/.test(version));
      if (!valid) fail('network', 'invalid_free_server', 'Check the server name and the modpack.');
      if (!allowance(free).allowed) refuse(free);
      if (kind === 'modrinth') {
        const known = listing.instances.find((i) => i.source.kind === 'modrinth' && i.source.versionId === version);
        if (known?.source.kind === 'modrinth' && known.source.projectId.endsWith('heavy')) {
          fail('pack', 'modpack_too_heavy', 'This modpack needs more memory than a free server has.', {
            requiredMemoryMb: String(HEAVY_MEMORY_MB),
            limitMemoryMb: String(MOCK_FREE_LIMITS.memoryMb.modded),
          });
        }
      }
      // Hold the place while the Engine works, so a second create is refused.
      free.server = cardOf({ name, kind: kind as View['kind'], freeStatus: 'creating', invited: false, deletion: null, usedBytes: 0 });
      await wait(MOCK_FREE_CREATE_MS);
      if (!free.server) fail('network', 'free_server_exists', 'The server was deleted while it was created.');
      free.server = cardOf({ name, kind: kind as View['kind'], freeStatus: 'starting', invited: true, deletion: { deletesAt: inDays(30), warned: false }, usedBytes: 0 });
      return answer(free.server);
    }
    case 'free_server_start': {
      const server = free.server ?? fail('network', 'invalid_free_server', "You don't have a free server.");
      if (server.disk.over) overDisk(server);
      const next = ['creating', 'deleting'].includes(server.freeStatus) ? server : restate(server, 'starting');
      free.server = next;
      return answer(next);
    }
    case 'free_server_delete':
      free.server = null;
      return true;
    case 'free_server_play': {
      const server = free.server ?? fail('pack', 'invite_unsupported', "This server can't be joined from the launcher yet.", { address: HOST });
      if (server.disk.over || server.freeStatus === 'stopped') overDisk(server);
      if (!server.joinable || server.invite === null) {
        fail('pack', 'invite_unsupported', `This server can't be joined from the launcher yet. Join with the address ${server.address ?? HOST}.`, { address: server.address ?? HOST });
      }
      if (server.freeStatus === 'offline') free.server = restate(server, 'starting');
      return join(server.invite).id;
    }
    default:
      return NOT_FREE;
  }
}

const overDisk = (server: FreeServerCard): never =>
  fail('network', 'free_server_over_disk', 'Your server is out of space, so it stays off.', {
    usedBytes: String(server.disk.usedBytes),
    limitBytes: String(server.disk.limitBytes),
  });

/** The same server with another Engine status (and the card's own `status` word that follows it). */
const restate = (server: FreeServerCard, freeStatus: string): FreeServerCard => ({
  ...server,
  freeStatus,
  status: freeStatus as FreeServerCard['status'],
});
