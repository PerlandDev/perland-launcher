// Mock Manage for `pnpm --filter ui dev:mock` and the tests: the servers the player runs (their free server and one team
// server), their state, players, settings, a console that echoes and a live log. Only answers while the Perland mock is
// signed in. Fake names only.
import { HOST } from './mockServers';
import type {
  AppEvent,
  FreeServerCard,
  LauncherError,
  ManageBan,
  ManageChangeResult,
  ManageDetail,
  ManageDiagnosis,
  ManagedServer,
  ManageListChange,
  ManagePlayers,
  ManageRef,
  ManageSaved,
  ManageSettings,
  ManageSettingsSave,
  ManageState,
  PerlandAccount,
} from './schema';

/** `?manage=`: `running` (the default, 3 playing), `asleep`, `stopped`, `crashed` (stopped, out of memory), `starting`
 *  (running after 3 s), `locked` (a save with more players answers `plan_feature_locked`), `modpack` (the version is
 *  locked), `motdCodes` (a MOTD with § codes, read only) and `none` (the player runs no server). Every managed server
 *  starts in the scenario's state. */
export type ManageScenario = 'running' | 'asleep' | 'stopped' | 'crashed' | 'starting' | 'locked' | 'modpack' | 'motdCodes' | 'none';

const SCENARIOS: readonly string[] = ['running', 'asleep', 'stopped', 'crashed', 'starting', 'locked', 'modpack', 'motdCodes', 'none'];
/** The `?manage=` preview param, or undefined for anything else (dev:mock only). */
export const manageScenario = (name: string | null): ManageScenario | undefined =>
  name !== null && SCENARIOS.includes(name) ? (name as ManageScenario) : undefined;

/** The free server's team. */
export const MOCK_FREE_TENANT = 't-free';
export const MOCK_TEAM_TENANT = 't-builders';
export const MOCK_TEAM_SERVER = 'Team survival';
/** How long a mock server takes to come up. */
export const MOCK_MANAGE_START_MS = 3000;
/** How often the open console gets a line. */
export const MOCK_LOG_EVERY_MS = 2000;

const PLAYING = ['Steve', 'Alex', 'Kai'];
const STOPS = ['stop', 'restart', 'end', 'shutdown'];
const MAX_NAMES = 50;

interface Server {
  ref: ManageRef;
  teamName: string;
  free: boolean;
  address: string;
  state: ManageState;
  /** While `starting`: when it is up. */
  upAt: number | null;
  crashed: boolean;
  online: string[];
  whitelist: string[];
  admins: string[];
  bans: ManageBan[];
  settings: ManageSettings;
  revision: number;
}

export interface MockManage {
  scenario: ManageScenario;
  /** The team server; the free one is built from the free-server mock as it stands. */
  team: Server;
  /** The free server's own Manage state, once it exists. */
  free: Server | null;
  console: { ref: ManageRef; timer: ReturnType<typeof setInterval> } | null;
  /** Where Manage in Perland opened the panel. */
  opened: { tenantId: string; stack: string | null }[];
  /** How events leave the mock; the backend sets it, tests may replace it. */
  emit: (event: AppEvent) => void;
}

function server(scenario: ManageScenario, ref: ManageRef, teamName: string, free: boolean, port: number): Server {
  const state: ManageState =
    scenario === 'asleep' ? 'asleep' : scenario === 'stopped' || scenario === 'crashed' ? 'stopped' : scenario === 'starting' ? 'starting' : 'running';
  return {
    ref,
    teamName,
    free,
    address: `${HOST}:${port}`,
    state,
    upAt: null,
    crashed: scenario === 'crashed',
    online: state === 'running' ? [...PLAYING] : [],
    whitelist: ['Steve', 'Alex'],
    admins: ['Steve'],
    bans: [{ name: 'Griefer', reason: 'Broke the spawn' }],
    settings: {
      revision: 'rev-1',
      motd: scenario === 'motdCodes' ? '§6Welcome §rto §bthe world' : `Welcome to ${ref.stack}`,
      motdReadOnly: scenario === 'motdCodes',
      mode: 'survival',
      difficulty: 'normal',
      maxPlayers: 10,
      pvp: true,
      enableWhitelist: false,
      version: scenario === 'modpack' ? '1.20.1' : '1.21.1',
      versionLocked: scenario === 'modpack',
      pendingRestart: false,
    },
    revision: 1,
  };
}

export function seedManage(scenario: ManageScenario = 'running', emit: (event: AppEvent) => void = () => {}): MockManage {
  return {
    scenario,
    team: server(scenario, { tenantId: MOCK_TEAM_TENANT, stack: MOCK_TEAM_SERVER }, 'Builders', false, 25600),
    free: null,
    console: null,
    opened: [],
    emit,
  };
}

function fail(category: LauncherError['category'], code: string, message: string, params: Record<string, string> = {}): never {
  throw { category, code, message, fix: null, detail: null, params } satisfies LauncherError;
}

/** The free server takes part once it exists (not while it is being created or deleted). */
const freeManaged = (card: FreeServerCard | null) => card !== null && !['creating', 'deleting'].includes(card.freeStatus);

/** The Manage reference the free server's card carries, or null. */
export function freeManageRef(manage: MockManage, card: FreeServerCard | null): ManageRef | null {
  return manage.scenario !== 'none' && card !== null && freeManaged(card) ? { tenantId: MOCK_FREE_TENANT, stack: card.name } : null;
}

/** The free server's Manage state, kept while its name stays. */
function syncFree(manage: MockManage, card: FreeServerCard | null) {
  if (!freeManaged(card) || manage.scenario === 'none') {
    manage.free = null;
    return;
  }
  const name = (card as FreeServerCard).name;
  if (manage.free?.ref.stack !== name) manage.free = server(manage.scenario, { tenantId: MOCK_FREE_TENANT, stack: name }, 'Perland', true, 25590);
}

/** A starting server that is up by now runs. */
function settle(s: Server) {
  if (s.state === 'starting' && s.upAt !== null && Date.now() >= s.upAt) {
    s.state = 'running';
    s.upAt = null;
    s.online = [...PLAYING];
  }
}

function every(manage: MockManage): Server[] {
  const all = [...(manage.free ? [manage.free] : []), ...(manage.scenario === 'none' ? [] : [manage.team])];
  all.forEach(settle);
  return all;
}

const view = (s: Server): ManagedServer => ({
  tenantId: s.ref.tenantId,
  teamName: s.teamName,
  stack: s.ref.stack,
  name: s.ref.stack,
  state: s.state,
  address: s.address,
  free: s.free,
});

/** The servers Your servers lists under `managed`: every one but the free server, which has its own card. */
export function managedForYourServers(manage: MockManage, account: PerlandAccount | null, card: FreeServerCard | null): ManagedServer[] {
  if (!account) return [];
  syncFree(manage, card);
  return every(manage)
    .filter((s) => !s.free)
    .map(view);
}

function start(s: Server) {
  s.state = 'starting';
  s.crashed = false;
  s.online = [];
  // The clock runs from now, so a screen opened later still sees the start.
  s.upAt = Date.now() + MOCK_MANAGE_START_MS;
}

function stop(s: Server) {
  s.state = 'stopped';
  s.upAt = null;
  s.online = [];
  s.settings.pendingRestart = false;
}

function detail(s: Server): ManageDetail {
  // `starting` from the scenario has no clock yet: it starts with the first read.
  if (s.state === 'starting' && s.upAt === null) s.upAt = Date.now() + MOCK_MANAGE_START_MS;
  settle(s);
  const running = s.state === 'running';
  return {
    tenantId: s.ref.tenantId,
    stack: s.ref.stack,
    state: s.state,
    address: s.address,
    online: running ? s.online.length : null,
    max: running ? (s.settings.maxPlayers ?? 20) : null,
    players: running ? [...s.online] : [],
    pendingRestart: s.settings.pendingRestart,
    version: s.settings.version,
    stopsAt: s.free && running ? new Date(Date.now() + 15 * 60_000).toISOString() : null,
  };
}

const diagnosis = (s: Server): ManageDiagnosis =>
  s.crashed
    ? { failed: true, oomKilled: true, cause: { code: 'out_of_memory', known: true, subject: null, params: {} } }
    : { failed: false, oomKilled: false, cause: null };

const players = (s: Server): ManagePlayers => ({
  running: s.state === 'running',
  online: s.state === 'running' ? [...s.online] : [],
  whitelist: [...s.whitelist],
  admins: [...s.admins],
  bans: s.bans.map((b) => ({ ...b })),
  unreadable: false,
});

/** The Engine's words per list: added, already there, removed, not there. */
const WORDS: Record<string, [string, string, string, string]> = {
  whitelist: ['added', 'already_listed', 'removed', 'not_listed'],
  admins: ['opped', 'already_op', 'deopped', 'not_op'],
  bans: ['banned', 'already_banned', 'pardoned', 'not_banned'],
};

function change(s: Server, c: ManageListChange): ManageChangeResult[] {
  const names = c.names.map((n) => n.trim()).filter((n) => n !== '');
  if (names.length === 0 || names.length > MAX_NAMES) fail('game', 'internal', 'Pick between 1 and 50 players.');
  if (c.list === 'online') {
    if (c.add) fail('game', 'internal', "That isn't a list players can be changed on.");
    return names.map((name) => {
      const on = s.online.some((n) => n.toLowerCase() === name.toLowerCase());
      s.online = s.online.filter((n) => n.toLowerCase() !== name.toLowerCase());
      return { name, status: on ? 'kicked' : 'failed', message: on ? null : 'That player is not online' };
    });
  }
  const [added, already, removed, missing] = WORDS[c.list] ?? fail('game', 'internal', "That isn't a list players can be changed on.");
  const has = (name: string) =>
    c.list === 'bans' ? s.bans.some((b) => b.name.toLowerCase() === name.toLowerCase()) : (c.list === 'whitelist' ? s.whitelist : s.admins).some((n) => n.toLowerCase() === name.toLowerCase());
  const put = (name: string, on: boolean) => {
    const keep = (n: string) => n.toLowerCase() !== name.toLowerCase();
    if (c.list === 'bans') {
      s.bans = on ? [...s.bans, { name, reason: null }] : s.bans.filter((b) => keep(b.name));
      if (on) s.online = s.online.filter(keep);
    } else if (c.list === 'whitelist') {
      s.whitelist = on ? [...s.whitelist, name] : s.whitelist.filter(keep);
    } else {
      s.admins = on ? [...s.admins, name] : s.admins.filter(keep);
    }
  };
  return names.map((name) => {
    const there = has(name);
    if (c.add !== there) put(name, c.add);
    return { name, status: c.add ? (there ? already : added) : there ? removed : missing, message: null };
  });
}

function save(manage: MockManage, s: Server, body: ManageSettingsSave): ManageSaved {
  if (body.revision !== s.settings.revision) fail('network', 'settings_changed', 'Someone changed these settings meanwhile.');
  if (manage.scenario === 'locked' && body.maxPlayers !== s.settings.maxPlayers) {
    fail('network', 'plan_feature_locked', "Your plan doesn't include this.", { field: 'maxPlayers' });
  }
  const running = s.state === 'running';
  const now = running && body.apply === 'now';
  const pending = running && body.apply === 'next_restart';
  s.revision += 1;
  s.settings = {
    ...s.settings,
    revision: `rev-${s.revision}`,
    motd: body.motd ?? s.settings.motd,
    motdReadOnly: body.motd === null ? s.settings.motdReadOnly : false,
    mode: body.mode,
    difficulty: body.difficulty,
    maxPlayers: body.maxPlayers,
    pvp: body.pvp,
    enableWhitelist: body.enableWhitelist,
    version: s.settings.versionLocked ? s.settings.version : (body.version ?? s.settings.version),
    pendingRestart: pending || s.settings.pendingRestart,
  };
  return { settings: { ...s.settings }, restarted: now, pending };
}

function runCommand(manage: MockManage, s: Server, command: string, confirm: boolean) {
  const text = command.trim();
  const word = text.split(/\s+/)[0]?.toLowerCase() ?? '';
  if (STOPS.includes(word) && !confirm) fail('game', 'command_needs_confirmation', 'This command stops the server.');
  if (s.state !== 'running') fail('network', 'server_not_running', "The server isn't running.");
  manage.emit({ type: 'server_log', tenantId: s.ref.tenantId, stack: s.ref.stack, lines: [`> ${text}`] });
  if (word === 'stop' || word === 'end' || word === 'shutdown') {
    stop(s);
    return { ok: true, output: 'Stopping the server' };
  }
  if (word === 'restart') {
    start(s);
    return { ok: true, output: 'Restarting the server' };
  }
  if (word === 'list') return { ok: true, output: `There are ${s.online.length} of a max of 10 players online: ${s.online.join(', ')}` };
  return { ok: true, output: `Ran: ${text}` };
}

const LOG_LINES = ['Saving the game', 'Saved the game', 'Steve has made the advancement [Stone Age]', '<Alex> anyone near spawn?'];

function openConsole(manage: MockManage, s: Server) {
  closeConsole(manage);
  let n = 0;
  const ref = { ...s.ref };
  const timer = setInterval(() => {
    const line = LOG_LINES[n % LOG_LINES.length];
    n += 1;
    const at = new Date().toTimeString().slice(0, 8);
    manage.emit({ type: 'server_log', tenantId: ref.tenantId, stack: ref.stack, lines: [`[${at}] [Server thread/INFO]: ${line}`] });
  }, MOCK_LOG_EVERY_MS);
  manage.console = { ref, timer };
}

/** Stops the live log, whichever server it follows (also at test teardown). */
export function closeConsole(manage: MockManage) {
  if (manage.console) clearInterval(manage.console.timer);
  manage.console = null;
}

export const NOT_MANAGE = Symbol('not a Manage command');

const signedOut = (): never => fail('auth', 'perland_signed_out', 'Sign in to Perland again.');

/** Answer a Manage command, or `NOT_MANAGE`. `card` is the free server as the free-server mock holds it now. */
export function handleManage(
  manage: MockManage,
  account: PerlandAccount | null,
  card: FreeServerCard | null,
  cmd: string,
  args: Record<string, unknown>,
): unknown {
  if (cmd !== 'managed_servers' && !cmd.startsWith('manage_')) return NOT_MANAGE;
  syncFree(manage, card);
  if (cmd === 'managed_servers') return account ? every(manage).map(view) : [];
  if (cmd === 'manage_open_panel') {
    manage.opened.push({ tenantId: String(args.tenantId ?? ''), stack: typeof args.stack === 'string' ? args.stack : null });
    return null;
  }
  if (cmd === 'manage_console_close') {
    const open = manage.console?.ref;
    if (open && open.tenantId === args.tenantId && open.stack === args.stack) closeConsole(manage);
    return null;
  }
  if (!account) signedOut();
  const s =
    every(manage).find((m) => m.ref.tenantId === args.tenantId && m.ref.stack === args.stack) ??
    fail('network', 'server_not_found', "That server isn't there any more.");
  switch (cmd) {
    case 'manage_detail':
      return detail(s);
    case 'manage_action':
      if (args.action === 'start' || args.action === 'restart') start(s);
      else if (args.action === 'stop') stop(s);
      else fail('game', 'internal', "That isn't something a server can do.");
      return null;
    case 'manage_diagnosis':
      return diagnosis(s);
    case 'manage_players':
      return players(s);
    case 'manage_players_change':
      return change(s, args.change as ManageListChange);
    case 'manage_settings':
      return { ...s.settings };
    case 'manage_settings_save':
      return save(manage, s, args.save as ManageSettingsSave);
    case 'manage_console':
      return runCommand(manage, s, String(args.command ?? ''), args.confirm === true);
    case 'manage_console_open':
      openConsole(manage, s);
      return null;
    default:
      return NOT_MANAGE;
  }
}
