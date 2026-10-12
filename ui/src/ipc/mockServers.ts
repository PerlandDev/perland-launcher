// Mock Perland servers for `pnpm --filter ui dev:mock` and the tests. One server instance is
// seeded from the contract fixtures, and the invite answers follow a scenario. Fake tokens only.
import joinPreviewFx from './contract/join_preview.json';
import serverInstanceFx from './contract/server_instance.json';
import serverStatusFx from './contract/server_status.json';
import { inviteCanonical } from './inviteLink';
import type { Instance, JoinPreview, LauncherError, ServerStatusView } from './schema';

export type ServerScenario = 'online' | 'vanilla' | 'pending' | 'whitelisted' | 'gone' | 'network' | 'modded-no-pack';

/** The invite the seeded server instance has. */
export const MOCK_INVITE = `https://play.example.com/${'A'.repeat(43)}`;
export const MOCK_SERVER_ID = 'survival';
export const NOT_MINE = Symbol('not a server command');

export interface MockServers {
  scenario: ServerScenario;
}

interface Listing {
  instances: Instance[];
}

const clone = <T,>(value: T): T => structuredClone(value);

function fail(category: LauncherError['category'], code: string, message: string, params: Record<string, string> = {}): never {
  throw { category, code, message, fix: null, detail: null, params } satisfies LauncherError;
}

function sorted(instances: Instance[]): Instance[] {
  return [...instances].sort((a, b) => a.name.localeCompare(b.name));
}

/** Add the `survival` server instance for `scenario`. */
export function seedServers(scenario: ServerScenario | undefined, listing: Listing): MockServers | null {
  if (!scenario) return null;
  const instance = clone(serverInstanceFx) as Instance;
  if (instance.server) {
    instance.server.gone = scenario === 'gone';
    instance.server.whitelisted = scenario === 'whitelisted';
  }
  if (scenario === 'vanilla') instance.game = { minecraft: '1.21.1', loader: null };
  listing.instances = sorted([...listing.instances, instance]);
  return { scenario };
}

function preview(servers: MockServers, existing: string | null): JoinPreview {
  const [modded, vanilla] = clone(joinPreviewFx) as JoinPreview[];
  const base = servers.scenario === 'vanilla' ? vanilla! : modded!;
  return {
    ...base,
    existing,
    whitelisted: servers.scenario === 'whitelisted',
    pack: base.pack && servers.scenario === 'pending' ? { ...base.pack, state: 'pending' } : base.pack,
  };
}

function status(servers: MockServers): ServerStatusView {
  const [online, gone] = clone(serverStatusFx) as ServerStatusView[];
  if (servers.scenario === 'gone') return gone!;
  const asleep = servers.scenario === 'vanilla';
  return {
    ...online!,
    status: asleep ? 'sleeping' : online!.status,
    players: asleep ? null : online!.players,
    whitelisted: servers.scenario === 'whitelisted',
  };
}

function perlandOf(instance: Instance): string | null {
  return instance.source.kind === 'perland' ? inviteCanonical(instance.source.invite) : null;
}

/** Answer a server command, or `NOT_MINE`. */
export function handleServer(servers: MockServers | null, listing: Listing, cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case 'join_preview':
    case 'join_add': {
      const canonical = inviteCanonical(String(args.link ?? '')) ?? fail('pack', 'invite_link_invalid', "That isn't a Perland invite link");
      if (!servers || servers.scenario === 'gone') fail('pack', 'invite_gone', "This invite link doesn't work anymore.");
      if (servers.scenario === 'network' || servers.scenario === 'modded-no-pack') {
        const address = 'play.example.com:25577';
        fail('pack', 'invite_unsupported', `This server can't be joined from the launcher yet. Join with the address ${address}`, { address });
      }
      const existing = listing.instances.find((i) => perlandOf(i) === canonical) ?? null;
      if (cmd === 'join_preview') return preview(servers, existing?.id ?? null);
      if (existing) return existing;
      const taken = new Set(listing.instances.map((i) => i.id));
      let id = MOCK_SERVER_ID;
      for (let n = 2; taken.has(id); n++) id = `${MOCK_SERVER_ID}-${n}`;
      const created: Instance = {
        ...(clone(serverInstanceFx) as Instance),
        id,
        source: { kind: 'perland', invite: canonical, engine: 'https://api.example.com/' },
      };
      listing.instances = sorted([...listing.instances, created]);
      return created;
    }
    case 'server_status':
      if (!servers) fail('pack', 'no_pack', "This instance doesn't come from a server invite");
      return status(servers);
    case 'server_relink': {
      const target = listing.instances.find((i) => i.id === args.id) ?? fail('disk', 'instance_not_found', 'gone');
      const canonical = inviteCanonical(String(args.link ?? '')) ?? fail('pack', 'invite_link_invalid', "That isn't a Perland invite link");
      const moved: Instance = {
        ...target,
        source: { kind: 'perland', invite: canonical, engine: 'https://api.example.com/' },
        updatePolicy: 'auto',
        server: target.server ? { ...target.server, gone: false } : null,
      };
      listing.instances = listing.instances.map((i) => (i.id === moved.id ? moved : i));
      if (servers) servers.scenario = 'online';
      return moved;
    }
    default:
      return NOT_MINE;
  }
}
