// Mock "Create a server" on a team for `pnpm --filter ui dev:mock` and the tests: the teams the player may create on, how much
// room each has, the Engine's version lists, and the create itself, which adds the server to Manage. Only answers while the
// Perland mock is signed in. Fake names only.
import { addTeamServer, MOCK_TEAM_TENANT, teamHasServer, type MockManage } from './mockManage';
import type {
  CreatableTeam,
  FreeServerSettings,
  Instance,
  LauncherError,
  PerlandAccount,
  TeamCreateRequest,
  TeamInstanceRequest,
  TeamRoom,
  TeamServerCreated,
  TeamServerType,
  TeamVersion,
} from './schema';

/** `?teams=`: `some` (the default: Builders with room, Night Shift with none, Starter Club whose plan has no modded servers)
 *  or `none` (the player is on no team, so the entry is today's). */
export type TeamsScenario = 'some' | 'none';
export const teamsScenario = (name: string | null): TeamsScenario | undefined => (name === 'some' || name === 'none' ? name : undefined);

/** A team with every server running: a server made there waits, stopped, for room. */
export const MOCK_FULL_TENANT = 't-night';
/** A team whose plan doesn't include modded servers: a Fabric, Forge, NeoForge or Quilt create is `plan_feature_locked`. */
export const MOCK_STARTER_TENANT = 't-starter';
/** How long a mock create takes. */
export const MOCK_TEAM_CREATE_MS = 800;

interface MockTeam extends CreatableTeam {
  room: TeamRoom;
  /** Every server already runs: a new one is made stopped. */
  full: boolean;
  /** The plan has no modded servers. */
  vanillaOnly: boolean;
}

const room = (max: number, cpu = 2): TeamRoom => ({
  memoryGb: Array.from({ length: max }, (_, i) => i + 1),
  defaultGb: Math.min(4, max),
  cpu,
});

const TEAMS: MockTeam[] = [
  { tenantId: MOCK_TEAM_TENANT, name: 'Builders', room: room(6), full: false, vanillaOnly: false },
  { tenantId: MOCK_FULL_TENANT, name: 'Night Shift', room: room(8), full: true, vanillaOnly: false },
  { tenantId: MOCK_STARTER_TENANT, name: 'Starter Club', room: room(2, 1), full: false, vanillaOnly: true },
];

const v = (id: string, stable = true, recommended = false): TeamVersion => ({ id, stable, recommended });
const MINECRAFT: Record<TeamServerType, TeamVersion[]> = {
  vanilla: [v('25w14a', false), v('1.21.5'), v('1.21.4'), v('1.21.1'), v('1.20.1')],
  paper: [v('1.21.5'), v('1.21.4'), v('1.21.1'), v('1.20.6')],
  fabric: [v('25w14a', false), v('1.21.5'), v('1.21.1'), v('1.20.1')],
  quilt: [v('1.21.1'), v('1.20.1')],
  forge: [v('1.21.1'), v('1.20.1')],
  neoforge: [v('1.21.1'), v('1.20.4')],
};
const LOADERS: Partial<Record<TeamServerType, (mc: string) => TeamVersion[]>> = {
  fabric: () => [v('0.16.14'), v('0.16.13'), v('0.17.0-beta.1', false)],
  quilt: () => [v('0.28.1'), v('0.29.0-beta.2', false)],
  forge: (mc) => (mc === '1.20.1' ? [v('47.4.0'), v('47.3.0', true, true)] : [v('52.1.0', true, true), v('52.0.9')]),
  neoforge: (mc) => (mc === '1.20.4' ? [v('20.4.237')] : [v('21.1.77'), v('21.1.80-beta', false)]),
};

const NAME_RULE = /^[A-Za-z0-9_-]{1,32}$/;

function fail(category: LauncherError['category'], code: string, message: string, params: Record<string, string> = {}): never {
  throw { category, code, message, fix: null, detail: null, params } satisfies LauncherError;
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export const NOT_TEAMS = Symbol('not a team create command');

const teamOf = (tenantId: unknown): MockTeam => TEAMS.find((t) => t.tenantId === tenantId) ?? fail('network', 'server_not_found', "That server wasn't found.");

/** The checks the app makes before anything is sent, then the Engine's answers. */
async function create(
  manage: MockManage,
  team: MockTeam,
  name: string,
  type: TeamServerType | null,
  memoryGb: number,
  settings: FreeServerSettings | null,
): Promise<TeamServerCreated> {
  const stack = name.trim();
  if (!NAME_RULE.test(stack)) fail('network', 'invalid_team_server', 'Use letters, numbers, - and _ only, up to 32.', { field: 'name' });
  if (!team.room.memoryGb.includes(memoryGb)) fail('network', 'invalid_team_server', 'Pick how much memory the server gets.', { field: 'ram' });
  if (settings?.maxPlayers !== undefined && (settings.maxPlayers < 1 || settings.maxPlayers > 1000)) {
    fail('network', 'invalid_team_server', "Check the server's options.", { field: 'settings.maxPlayers' });
  }
  await wait(MOCK_TEAM_CREATE_MS);
  if (team.vanillaOnly && type !== null && type !== 'vanilla' && type !== 'paper') {
    fail('network', 'plan_feature_locked', "Your plan doesn't include this.", { field: 'type' });
  }
  if (teamHasServer(manage, team.tenantId, stack)) fail('network', 'server_name_taken', 'Your team already has a server with that name.', { field: 'name' });
  const address = addTeamServer(manage, { tenantId: team.tenantId, stack }, team.name, !team.full);
  return {
    tenantId: team.tenantId,
    stack,
    started: !team.full,
    notStartedReason: team.full ? 'quota_exceeded' : null,
    address,
  };
}

/** Answer a team create command, or `NOT_TEAMS`. `find` reads an instance by id. */
export async function handleTeams(
  scenario: TeamsScenario,
  manage: MockManage,
  account: PerlandAccount | null,
  find: (id: string) => Instance | undefined,
  cmd: string,
  args: Record<string, unknown>,
): Promise<unknown> {
  if (!['creatable_teams', 'team_room', 'team_versions', 'team_create', 'team_check', 'team_create_from_instance'].includes(cmd)) return NOT_TEAMS;
  if (cmd === 'creatable_teams') return account && scenario === 'some' ? TEAMS.map(({ tenantId, name }) => ({ tenantId, name })) : [];
  if (!account) fail('auth', 'perland_signed_out', 'Sign in to Perland again.');
  switch (cmd) {
    case 'team_room':
      return { ...teamOf(args.tenantId).room };
    case 'team_versions': {
      teamOf(args.tenantId);
      const type = args.serverType as TeamServerType;
      const minecraft = typeof args.minecraft === 'string' ? args.minecraft : null;
      if (minecraft === null) return MINECRAFT[type] ?? fail('network', 'invalid_team_server', 'Pick a server type.', { field: 'type' });
      return LOADERS[type]?.(minecraft) ?? [];
    }
    case 'team_check': {
      teamOf(args.tenantId);
      const instance = find(String(args.id)) ?? fail('disk', 'instance_not_found', 'gone');
      return {
        name: instance.name,
        mods: 14,
        serverMods: 12,
        requiredMb: 2560,
        limitMb: teamOf(args.tenantId).room.memoryGb.length * 1024,
        notOnModrinth: [],
        settings: 52,
      };
    }
    case 'team_create': {
      const r = args.request as TeamCreateRequest;
      return create(manage, teamOf(r.tenantId), r.name, r.type, r.memoryGb, r.settings);
    }
    case 'team_create_from_instance': {
      const r = args.request as TeamInstanceRequest;
      const instance = find(r.id) ?? fail('disk', 'instance_not_found', 'gone');
      if (instance.source.kind === 'perland' || instance.server !== null) {
        fail('network', 'invalid_team_server', "This instance belongs to a server, so it can't become another one.");
      }
      // A Modrinth pack decides its own type; an instance's loader is its type.
      const type: TeamServerType | null = instance.source.kind === 'modrinth' ? null : (instance.game.loader?.kind ?? 'vanilla');
      return create(manage, teamOf(r.tenantId), r.name, type, r.memoryGb, r.settings);
    }
    default:
      return NOT_TEAMS;
  }
}
