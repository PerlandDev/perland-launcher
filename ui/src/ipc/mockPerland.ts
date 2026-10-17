// Mock Perland account for `pnpm --filter ui dev:mock` and the tests: the sign-in methods, the
// Discord and email flows, and the player's Your servers set. Fake tokens and addresses only.
import { inviteCanonical } from './inviteLink';
import { inviteOf, joinedInstance, MOCK_PUBLIC_SERVERS } from './mockServers';
import type { AppEvent, Instance, LauncherError, PerlandAccount, PerlandStatus, PublicServerCard, YourServerCard, YourServersPage } from './schema';

/** `?perland=`: `off` closes accounts; `signedOut` (the default) offers Discord and email; `signedIn` has a player with
 *  one owner, one invited and two followed servers; `empty` is signed in with none; `emailOnly` offers only email. */
export type PerlandScenario = 'off' | 'signedOut' | 'signedIn' | 'empty' | 'emailOnly';

const SCENARIOS: readonly string[] = ['off', 'signedOut', 'signedIn', 'empty', 'emailOnly'];
/** The `?perland=` preview param, or undefined for anything else (dev:mock only). */
export const perlandScenario = (name: string | null): PerlandScenario | undefined =>
  name !== null && SCENARIOS.includes(name) ? (name as PerlandScenario) : undefined;

/** The email code the mock accepts. */
export const MOCK_EMAIL_CODE = '123456';
/** How long the mock Discord sign-in waits on "Finish in your browser". */
export const MOCK_DISCORD_WAIT_MS = 1500;
const ATTEMPTS = 5;
const RESEND_IN = 60;

export interface MockPerland {
  scenario: PerlandScenario;
  account: PerlandAccount | null;
  servers: YourServerCard[];
  /** Wrong codes left before the current code is used up. */
  attemptsLeft: number;
  /** Ends the Discord step waiting in the browser, as the player's Cancel does. */
  cancel: (() => void) | null;
}

interface Listing {
  instances: Instance[];
}

const STEVE: PerlandAccount = { name: 'Steve', email: 'steve@example.com', discord: true, newPlayer: false };

/** The tokens of the servers Your servers has that the public list doesn't. */
const OWNER = 'O';
const INVITED = 'I';
const QUIET = 'K';
/** The followed server that is also in the public list. */
const SHARED = 'F';

const card = (letter: string, over: Partial<PublicServerCard>): PublicServerCard => ({
  token: letter.repeat(43),
  invite: inviteOf(letter),
  joinable: true,
  address: 'play.example.com:25580',
  name: '',
  iconVersion: null,
  game: { version: '1.21.1', loader: null, loaderVersion: null },
  modded: false,
  status: 'online',
  players: null,
  whitelisted: false,
  pack: null,
  categories: [],
  language: 'en',
  region: null,
  instance: null,
  ...over,
});

/** Owner, invited, followed. The followed `SHARED` one is the public list's own card, token and invite included. */
function seedServers(): YourServerCard[] {
  const shared = MOCK_PUBLIC_SERVERS.find((c) => c.token.startsWith(SHARED)) as PublicServerCard;
  return [
    { ...card(OWNER, { name: 'My world', status: 'offline', address: 'play.example.com:25580' }), relation: 'owner', following: false },
    {
      ...card(INVITED, { name: 'Friends', whitelisted: true, players: { online: 2, max: 8 }, address: 'play.example.com:25581' }),
      relation: 'invited',
      following: true,
    },
    { ...structuredClone(shared), relation: 'followed', following: true },
    { ...card(QUIET, { name: 'Quiet Cove', status: 'sleeping', address: 'play.example.com:25582' }), relation: 'followed', following: true },
  ];
}

export function seedPerland(scenario: PerlandScenario = 'signedOut'): MockPerland {
  const signedIn = scenario === 'signedIn' || scenario === 'empty';
  return {
    scenario,
    account: signedIn ? { ...STEVE } : null,
    servers: scenario === 'empty' ? [] : seedServers(),
    attemptsLeft: ATTEMPTS,
    cancel: null,
  };
}

function fail(category: LauncherError['category'], code: string, message: string, params: Record<string, string> = {}): never {
  throw { category, code, message, fix: null, detail: null, params } satisfies LauncherError;
}

const closed = (): never => fail('network', 'player_accounts_closed', "Perland accounts aren't open yet.");

const methodsOf = (scenario: PerlandScenario): string[] =>
  scenario === 'off' ? [] : scenario === 'emailOnly' ? ['email'] : ['discord', 'email'];

export const NOT_PERLAND = Symbol('not a Perland command');

const RELATIONS = ['owner', 'invited', 'followed'] as const;

/** Your servers in the Engine's order (owner, invited, followed; each by name), with `instance` filled in like the public list does. */
function page(perland: MockPerland, listing: Listing): YourServersPage {
  const rank = (c: YourServerCard) => RELATIONS.indexOf(c.relation);
  const servers = [...perland.servers].sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name));
  return { servers: servers.map((c) => ({ ...structuredClone(c), instance: joinedInstance(c, listing) })) };
}

const find = (perland: MockPerland, invite: unknown) => {
  const canonical = inviteCanonical(String(invite ?? ''));
  return canonical === null ? undefined : perland.servers.find((c) => c.invite !== null && inviteCanonical(c.invite) === canonical);
};

/** A public card for `invite`, or undefined: follow and joined are only for servers on the public list. */
const publicCard = (invite: unknown) => {
  const canonical = inviteCanonical(String(invite ?? ''));
  return canonical === null ? undefined : MOCK_PUBLIC_SERVERS.find((c) => c.invite !== null && inviteCanonical(c.invite) === canonical);
};

const wait = (ms: number, perland: MockPerland) =>
  new Promise<boolean>((resolve) => {
    const timer = setTimeout(() => {
      perland.cancel = null;
      resolve(true);
    }, ms);
    perland.cancel = () => {
      clearTimeout(timer);
      perland.cancel = null;
      resolve(false);
    };
  });

/** Answer a Perland account or Your servers command, or `NOT_PERLAND`. */
export async function handlePerland(
  perland: MockPerland,
  listing: Listing,
  cmd: string,
  args: Record<string, unknown>,
  emit: (event: AppEvent) => void,
): Promise<unknown> {
  const on = perland.scenario !== 'off';
  switch (cmd) {
    case 'perland_status':
      return { enabled: on, methods: methodsOf(perland.scenario), account: on ? perland.account : null } satisfies PerlandStatus;
    case 'perland_sign_in_discord': {
      if (!on) closed();
      if (!methodsOf(perland.scenario).includes('discord')) {
        fail('network', 'sign_in_unavailable', "That way of signing in isn't available.", { method: 'discord' });
      }
      emit({ type: 'perland_sign_in', phase: 'waiting' });
      if (!(await wait(MOCK_DISCORD_WAIT_MS, perland))) return null;
      emit({ type: 'perland_sign_in', phase: 'exchanging' });
      perland.account = { ...STEVE, newPlayer: true };
      return perland.account;
    }
    case 'perland_sign_in_reopen':
      return null;
    case 'perland_sign_in_cancel':
      perland.cancel?.();
      return null;
    case 'perland_email_start': {
      if (!on) closed();
      if (!methodsOf(perland.scenario).includes('email')) fail('network', 'sign_in_unavailable', "That way of signing in isn't available.", { method: 'email' });
      if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(String(args.email ?? '').trim())) fail('auth', 'invalid_email', 'Check the email address.');
      perland.attemptsLeft = ATTEMPTS;
      return { resendIn: RESEND_IN };
    }
    case 'perland_email_verify': {
      if (!on) closed();
      const email = String(args.email ?? '').trim();
      if (String(args.code ?? '').replace(/\s+/g, '') !== MOCK_EMAIL_CODE) {
        perland.attemptsLeft = Math.max(0, perland.attemptsLeft - 1);
        fail('auth', 'invalid_code', "That code isn't right.", { attemptsLeft: String(perland.attemptsLeft) });
      }
      perland.account = { name: email, email, discord: false, newPlayer: true };
      return perland.account;
    }
    case 'perland_sign_out':
      perland.account = null;
      return null;
    case 'your_servers':
      return on && perland.account ? page(perland, listing) : null;
    case 'your_servers_follow': {
      if (!on || !perland.account) return null;
      const known = publicCard(args.invite);
      if (known && !find(perland, args.invite)) perland.servers.push({ ...structuredClone(known), relation: 'followed', following: true });
      return null;
    }
    case 'your_servers_unfollow': {
      if (!on || !perland.account) return null;
      const entry = find(perland, args.invite);
      // Only a followed server can be unfollowed; an owner's or an invite's stays.
      if (entry?.relation === 'followed') perland.servers = perland.servers.filter((c) => c !== entry);
      return null;
    }
    case 'your_servers_joined': {
      if (!on || !perland.account) return null;
      const known = publicCard(args.invite);
      if (known && !find(perland, args.invite)) perland.servers.push({ ...structuredClone(known), relation: 'invited', following: true });
      return null;
    }
    default:
      return NOT_PERLAND;
  }
}
