// Mock Perland servers for `pnpm --filter ui dev:mock` and the tests. One server instance is
// seeded from the contract fixtures, and the invite answers follow a scenario. Fake tokens only.
import joinPreviewFx from './contract/join_preview.json';
import serverInstanceFx from './contract/server_instance.json';
import serverStatusFx from './contract/server_status.json';
import { inviteCanonical } from './inviteLink';
import type { Instance, JoinPreview, LauncherError, ListQuery, LoaderKind, NewsPostView, PublicServerCard, PublicServerPage, ServerNews, ServerStatusView } from './schema';

export type ServerScenario =
  | 'online'
  | 'starting'
  | 'offline'
  | 'vanilla'
  | 'pending'
  | 'whitelisted'
  | 'gone'
  | 'network'
  | 'modded-no-pack'
  /** The public server list isn't open yet: `servers_list` answers `server_list_closed`. */
  | 'closed'
  /** The Engine has the news turned off: `server_news` answers null for every server. */
  | 'news-off';

/** The invite the seeded server instance has. */
export const MOCK_INVITE = `https://play.example.com/${'A'.repeat(43)}`;
export const MOCK_SERVER_ID = 'survival';
/** A real 64 × 64 server icon (original pixel art, terraced plots at dusk, generated with Node), so the
 *  mock shows the icon at its integer pixel scale. The contract fixture's icon is only a PNG signature. */
export const MOCK_SERVER_ICON =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAYAAACqaXHeAAABdElEQVR42u2YPW7CQBCF5xqhI6IykpsoFFSR6FxwgDRcIXUOkCLHQjQoXCAnoIaCdpCRiIiz4B+88YzfK54sQLPr+fxmvIOMBqkiSwiAAP5vs8Nu6weAxZtlCRBA++VHB8ADSB8n2lS5re6JtyDxngAUAP3+vCm3AKqUUlnyTSGIyye/X/3WHRDEVfLFxG+AaBVAl93eBAATDihLvgChcwBtuSY6gOl4qpbVFEDV9QnADYCaTZAA+gLgD4QKB6E6a7sAEIRwRXXXdQOgCoQma7oCEEMEkD2/KLIIgAAIgAAIABrA8ONLkUUA71mib7NE8+s8fdDLz5fX/LfQ997jpI9J1YmTvj/hsjhBsnsoTtabRC0rNgzzAGI7Q1Br/xwnqLX/4wC0934xTpAPQVcd0OYmunwyLYlN3DwA5EPQqQeg1v45jtMg/DSorwvtQmamwa4AmJkGkQ9BwR6ABoPTIKdBToOcBhW5EQryX+JBB3AaBINxBGzHqnIKs/OSAAAAAElFTkSuQmCC';
export const NOT_MINE = Symbol('not a server command');

const HOST = 'play.example.com';
const inviteOf = (letter: string) => `https://${HOST}/${letter.repeat(43)}`;
const loaderOf = (loader: string | null, version: string | null) => ({ version: null as string | null, loader, loaderVersion: version });

/** The public list's mock servers, oldest first (Newest sorts from the end). Tokens are fake: one letter, 43 times.
 *  They cover online with players, full, sleeping, starting, whitelisted, vanilla, modded without a pack
 *  (not joinable), two Minecraft versions, both languages and three regions. */
export const MOCK_PUBLIC_SERVERS: PublicServerCard[] = [
  {
    token: 'A'.repeat(43),
    invite: inviteOf('A'),
    joinable: true,
    address: `${HOST}:25566`,
    name: 'Survival',
    iconVersion: '1a2b3c4d5e6f7a8b',
    game: { ...loaderOf('fabric', '0.16.14'), version: '1.20.1' },
    modded: true,
    status: 'online',
    players: { online: 7, max: 20 },
    whitelisted: false,
    pack: { name: 'Survival Pack', version: '1.0.4', modCount: 142, state: 'ready' },
    categories: ['survival', 'modded-tech'],
    language: 'en',
    region: 'eu',
    instance: null,
  },
  {
    token: 'B'.repeat(43),
    invite: inviteOf('B'),
    joinable: true,
    address: `${HOST}:25567`,
    name: 'Creative Plots',
    iconVersion: '2b3c4d5e6f7a8b9c',
    game: { ...loaderOf(null, null), version: '1.21.1' },
    modded: false,
    status: 'online',
    players: { online: 20, max: 20 },
    whitelisted: false,
    pack: null,
    categories: ['creative'],
    language: 'es',
    region: 'latam',
    instance: null,
  },
  {
    token: 'C'.repeat(43),
    invite: inviteOf('C'),
    joinable: true,
    address: `${HOST}:25568`,
    name: 'Sleepy Hollow',
    iconVersion: null,
    game: { ...loaderOf(null, null), version: '1.21.1' },
    modded: false,
    status: 'sleeping',
    players: null,
    whitelisted: false,
    pack: null,
    categories: ['smp', 'roleplay'],
    language: 'en',
    region: 'us',
    instance: null,
  },
  {
    token: 'D'.repeat(43),
    invite: inviteOf('D'),
    joinable: true,
    address: `${HOST}:25569`,
    name: 'Skyblock Dawn',
    iconVersion: '4d5e6f7a8b9c0d1e',
    game: { ...loaderOf(null, null), version: '1.20.1' },
    modded: false,
    status: 'starting',
    players: null,
    whitelisted: false,
    pack: null,
    categories: ['skyblock'],
    language: 'en',
    region: 'eu',
    instance: null,
  },
  {
    token: 'E'.repeat(43),
    invite: inviteOf('E'),
    joinable: true,
    address: `${HOST}:25570`,
    name: 'The Whitelist Club',
    iconVersion: null,
    game: { ...loaderOf(null, null), version: '1.21.1' },
    modded: false,
    status: 'online',
    players: { online: 3, max: 10 },
    whitelisted: true,
    pack: null,
    categories: ['smp'],
    language: 'en',
    region: 'eu',
    instance: null,
  },
  {
    token: 'F'.repeat(43),
    invite: inviteOf('F'),
    joinable: true,
    address: `${HOST}:25571`,
    name: 'Pure Vanilla',
    iconVersion: '6f7a8b9c0d1e2f3a',
    game: { ...loaderOf(null, null), version: '1.20.1' },
    modded: false,
    status: 'online',
    players: { online: 12, max: 50 },
    whitelisted: false,
    pack: null,
    categories: ['survival', 'pvp'],
    language: 'en',
    region: 'us',
    instance: null,
  },
  {
    token: 'G'.repeat(43),
    invite: null,
    joinable: false,
    address: `${HOST}:25572`,
    name: 'Tech Forge',
    iconVersion: null,
    game: { ...loaderOf('forge', '47.3.0'), version: '1.20.1' },
    modded: true,
    status: 'online',
    players: { online: 5, max: 30 },
    whitelisted: false,
    pack: null,
    categories: ['modded-tech'],
    language: 'en',
    region: 'eu',
    instance: null,
  },
  {
    token: 'H'.repeat(43),
    invite: inviteOf('H'),
    joinable: true,
    address: `${HOST}:25573`,
    name: 'Aventura Épica',
    iconVersion: '8b9c0d1e2f3a4b5c',
    game: { ...loaderOf('fabric', '0.16.14'), version: '1.21.1' },
    modded: true,
    status: 'online',
    players: { online: 2, max: 30 },
    whitelisted: false,
    pack: { name: 'Aventura Pack', version: '2.1.0', modCount: 64, state: 'ready' },
    categories: ['adventure-rpg', 'magic'],
    language: 'es',
    region: 'latam',
    instance: null,
  },
];

const HOUR = 3_600_000;
const DAY = 24 * HOUR;
const MOCK_NEWS_IMAGE_VERSION = 'v1';
/** Mock server A's older recaps: with its 7 other posts, 24 in all, so a 20-post page leaves 4 for Show more. */
const OLD_RECAPS = 17;

/** An id that looks like the Engine's UUIDs, one per server letter and post number. */
const newsId = (letter: string, n: number) => `${(letter.charCodeAt(0).toString(16) + '00000000').slice(0, 8)}-0000-4000-8000-${String(n).padStart(12, '0')}`;

/** The posts a mock server has, relative to `at`, newest first. A: the seeded instance's, with every kind of post; C: none. */
function newsPosts(letter: string, at: number): NewsPostView[] {
  const iso = (ms: number) => new Date(ms).toISOString();
  const post = (n: number, over: Partial<NewsPostView> & Pick<NewsPostView, 'type' | 'title' | 'createdAt'>): NewsPostView => ({
    id: newsId(letter, n),
    body: '',
    startsAt: null,
    endsAt: null,
    imageVersion: null,
    modpack: null,
    ...over,
  });
  const contest = (n: number, startsAt: number, createdAt: number) =>
    post(n, {
      type: 'event',
      title: 'Build contest',
      body: 'Bring your best build.\nThe theme is castles.',
      startsAt: iso(startsAt),
      endsAt: iso(startsAt + 2 * HOUR),
      createdAt: iso(createdAt),
    });
  const picture = (n: number, createdAt: number) =>
    post(n, { type: 'update', title: 'New spawn is live', body: 'Come and see it.\nThe old spawn stays as a museum.', imageVersion: MOCK_NEWS_IMAGE_VERSION, createdAt: iso(createdAt) });
  const modpack = (n: number, version: string, createdAt: number, changes: boolean) =>
    post(n, {
      type: 'modpack',
      title: `Modpack ${version}`,
      createdAt: iso(createdAt),
      modpack: changes
        ? {
            added: { count: 3, names: ['Sodium', 'Iris Shaders', 'Lithium'] },
            removed: { count: 1, names: ['Zoomify'] },
            updated: { count: 2, names: ['Fabric API', 'Mod Menu'] },
          }
        : null,
    });
  switch (letter) {
    case 'A':
      return [
        picture(1, at - 2 * HOUR),
        modpack(2, '1.0.4', at - 1 * DAY, true),
        contest(3, at + 26 * HOUR, at - 3 * DAY),
        post(4, { type: 'update', title: 'Rules reminder', body: '<b>bold</b>', createdAt: iso(at - 5 * DAY) }),
        modpack(5, '1.0.3', at - 8 * DAY, false),
        post(6, { type: 'update', title: 'Weekend recap', body: 'Thanks for playing!', createdAt: iso(at - 12 * DAY) }),
        post(7, { type: 'update', title: 'Server moved to a faster machine', createdAt: iso(at - 20 * DAY) }),
        // Older weekly recaps, so the dialog's 20-post pages end in Show more.
        ...Array.from({ length: OLD_RECAPS }, (_, i) =>
          post(8 + i, { type: 'update', title: 'Weekend recap', body: `Week ${OLD_RECAPS - i} is in the books.`, createdAt: iso(at - (27 + 7 * i) * DAY) }),
        ),
      ];
    case 'B':
      return [
        post(1, { type: 'event', title: 'Plot party', startsAt: iso(at - 30 * 60_000), createdAt: iso(at - 1 * DAY) }),
        picture(2, at - 4 * DAY),
      ];
    case 'D':
      return [post(1, { type: 'update', title: 'Islands reset on Friday', body: 'Back up what you want to keep.', createdAt: iso(at - 9 * DAY) })];
    case 'E':
      return [contest(1, at + 3 * DAY, at - 2 * DAY)];
    case 'F':
      return [post(1, { type: 'update', title: 'Season 4 starts', createdAt: iso(at - 6 * HOUR) }), contest(2, at - 6 * DAY, at - 8 * DAY)];
    case 'H':
      return [
        post(1, { type: 'update', title: 'Nueva aventura', body: 'Hay un jefe nuevo.\nBusca la torre.', createdAt: iso(at - 3 * DAY) }),
        modpack(2, '2.1.0', at - 4 * DAY, true),
      ];
    default:
      return [];
  }
}

/** Per mock backend: the moment its posts are relative to, and the seen markers (the seeded instance starts with an old one, so its dot shows). */
const newsStates = new WeakMap<object, { at: number; seen: Map<string, string> }>();

function newsState(listing: object) {
  let state = newsStates.get(listing);
  if (!state) {
    const at = Math.floor(Date.now() / 1000) * 1000;
    state = { at, seen: new Map([[inviteCanonical(MOCK_INVITE) as string, new Date(at - 3 * DAY).toISOString()]]) };
    newsStates.set(listing, state);
  }
  return state;
}

/** One page of a mock server's news, the way the Engine answers: newest first, `before` is an offset, 1 to 50 a page. */
function serverNews(listing: object, args: Record<string, unknown>): ServerNews | null {
  const canonical = inviteCanonical(String(args.invite ?? ''));
  const card = cardOf(canonical);
  if (!card || !canonical) return null;
  const state = newsState(listing);
  const all = newsPosts(card.token[0] ?? '', state.at).sort((a, b) => b.createdAt.localeCompare(a.createdAt));
  const limit = Math.min(50, Math.max(1, Number(args.limit) || 20));
  const offset = typeof args.before === 'string' && /^\d+$/.test(args.before) ? Number(args.before) : 0;
  const end = offset + limit;
  return { posts: all.slice(offset, end), nextBefore: end < all.length ? String(end) : null, seen: state.seen.get(canonical) ?? null };
}

const PAGE = 50;

const byVersionDesc = (a: string, b: string) => b.localeCompare(a, 'en', { numeric: true });
const uniqueSorted = (values: (string | null)[], compare?: (a: string, b: string) => number) =>
  [...new Set(values.filter((v): v is string => v !== null))].sort(compare);

const hasRoom = (card: PublicServerCard) => !card.players || card.players.max === null || card.players.online < card.players.max;

/** The card for a canonical invite. */
const cardOf = (canonical: string | null) =>
  canonical === null ? undefined : MOCK_PUBLIC_SERVERS.find((c) => c.invite !== null && inviteCanonical(c.invite) === canonical);

/** Does Play on this instance wake its server first? Only a Perland server that was asleep or starting. */
export function wakesOnPlay(instance: Instance): boolean {
  if (instance.source.kind !== 'perland') return false;
  const card = cardOf(inviteCanonical(instance.source.invite));
  return card?.status === 'sleeping' || card?.status === 'starting';
}

/** One page of the public list for `query`, the way the Engine answers: filters, sort, 50 a page. Facets cover the whole set. */
function publicPage(query: ListQuery, listing: Listing): PublicServerPage {
  const joined = (card: PublicServerCard) =>
    listing.instances.find((i) => i.source.kind === 'perland' && card.invite !== null && inviteCanonical(i.source.invite) === inviteCanonical(card.invite))?.id ?? null;
  const players = (c: PublicServerCard) => c.players?.online ?? 0;
  const matching = MOCK_PUBLIC_SERVERS.map((card, index) => ({ card, index }))
    .filter(
      ({ card }) =>
        (query.modded === null || card.modded === query.modded) &&
        (!query.minecraft || card.game.version === query.minecraft) &&
        (!query.hasRoom || hasRoom(card)) &&
        (!query.category || card.categories.includes(query.category)) &&
        (!query.language || card.language === query.language) &&
        (!query.region || card.region === query.region),
    )
    .sort((a, b) =>
      query.sort === 'newest'
        ? b.index - a.index
        : query.sort === 'name'
          ? a.card.name.localeCompare(b.card.name)
          : players(b.card) - players(a.card) || a.card.name.localeCompare(b.card.name),
    );
  return {
    servers: matching.slice(query.offset, query.offset + PAGE).map(({ card }) => ({ ...clone(card), instance: joined(card) })),
    total: matching.length,
    facets: {
      categories: uniqueSorted(MOCK_PUBLIC_SERVERS.flatMap((c) => c.categories)),
      languages: uniqueSorted(MOCK_PUBLIC_SERVERS.map((c) => c.language)),
      minecraftVersions: uniqueSorted(
        MOCK_PUBLIC_SERVERS.map((c) => c.game.version),
        byVersionDesc,
      ),
      regions: uniqueSorted(MOCK_PUBLIC_SERVERS.map((c) => c.region)),
    },
  };
}

function addressOf(card: PublicServerCard): { host: string; port: number } | null {
  const at = card.address?.lastIndexOf(':') ?? -1;
  if (!card.address || at < 0) return null;
  return { host: card.address.slice(0, at), port: Number(card.address.slice(at + 1)) };
}

/** The server instance Add makes from a public card. */
function instanceFromCard(card: PublicServerCard, id: string, canonical: string): Instance {
  const base = clone(serverInstanceFx) as Instance;
  return {
    ...base,
    id,
    name: card.name,
    source: { kind: 'perland', invite: canonical, engine: 'https://api.example.com/' },
    game: {
      minecraft: card.game.version ?? '1.21.1',
      loader: card.game.loader ? { kind: card.game.loader as LoaderKind, version: card.game.loaderVersion ?? '' } : null,
    },
    server: { address: addressOf(card), name: card.name, accent: null, whitelisted: card.whitelisted, lastStatus: card.status, lastSeen: null, gone: false },
  };
}

function cardPreview(card: PublicServerCard, existing: string | null): JoinPreview {
  return {
    host: HOST,
    name: card.name,
    motd: card.name,
    kind: 'stack',
    status: card.status,
    players: card.players,
    game: card.game,
    pack: card.pack,
    whitelisted: card.whitelisted,
    team: null,
    address: addressOf(card),
    icon: card.iconVersion ? MOCK_SERVER_ICON : null,
    existing,
  };
}

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
  if (scenario === 'closed') return { scenario };
  const instance = clone(serverInstanceFx) as Instance;
  if (instance.server) {
    instance.server.gone = scenario === 'gone';
    instance.server.whitelisted = scenario === 'whitelisted';
  }
  if (scenario === 'vanilla') instance.game = { minecraft: '1.21.1', loader: null };
  listing.instances = sorted([...listing.instances, instance]);
  return { scenario };
}

/** The scenarios that pin the server's live status, or null to keep the fixture's. */
const pinnedStatus = (scenario: ServerScenario): 'starting' | 'offline' | null =>
  scenario === 'starting' || scenario === 'offline' ? scenario : null;

function preview(servers: MockServers, existing: string | null): JoinPreview {
  const [modded, vanilla] = clone(joinPreviewFx) as JoinPreview[];
  const base = servers.scenario === 'vanilla' ? vanilla! : modded!;
  const pinned = pinnedStatus(servers.scenario);
  return {
    ...base,
    existing,
    icon: base.icon ? MOCK_SERVER_ICON : null,
    status: pinned ?? base.status,
    players: pinned ? null : base.players,
    whitelisted: servers.scenario === 'whitelisted',
    pack: base.pack && servers.scenario === 'pending' ? { ...base.pack, state: 'pending' } : base.pack,
  };
}

function status(servers: MockServers): ServerStatusView {
  const [online, gone] = clone(serverStatusFx) as ServerStatusView[];
  if (servers.scenario === 'gone') return gone!;
  const asleep = servers.scenario === 'vanilla';
  const pinned = pinnedStatus(servers.scenario);
  return {
    ...online!,
    status: asleep ? 'sleeping' : (pinned ?? online!.status),
    players: asleep || pinned ? null : online!.players,
    whitelisted: servers.scenario === 'whitelisted',
  };
}

function perlandOf(instance: Instance): string | null {
  return instance.source.kind === 'perland' ? inviteCanonical(instance.source.invite) : null;
}

/** Answer a server command, or `NOT_MINE`. */
export function handleServer(servers: MockServers | null, listing: Listing, cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case 'servers_list':
      if (servers?.scenario === 'closed') fail('network', 'server_list_closed', "The server list isn't open yet. Check back soon.", { host: HOST });
      return publicPage(args.query as ListQuery, listing);
    case 'servers_icon': {
      const card = MOCK_PUBLIC_SERVERS.find((c) => c.token === args.token);
      return card?.iconVersion && card.iconVersion === args.iconVersion ? MOCK_SERVER_ICON : null;
    }
    case 'server_news':
      return servers?.scenario === 'news-off' ? null : clone(serverNews(listing, args));
    case 'server_news_opened': {
      const canonical = inviteCanonical(String(args.invite ?? ''));
      const marks = newsState(listing).seen;
      const newest = String(args.newest ?? '');
      if (canonical && (!marks.has(canonical) || newest > (marks.get(canonical) as string))) marks.set(canonical, newest);
      return null;
    }
    case 'news_image': {
      const page = serverNews(listing, { invite: args.invite, limit: 50 });
      const known = page?.posts.some((p) => p.id === args.postId && p.imageVersion === args.imageVersion);
      return known ? MOCK_SERVER_ICON : null;
    }
    case 'join_preview':
    case 'join_add': {
      const canonical = inviteCanonical(String(args.link ?? '')) ?? fail('pack', 'invite_link_invalid', "That isn't a Perland invite link");
      // Without an invite scenario, a card of the public list joins on its own; with one, the scenario answers every invite.
      const card = servers ? undefined : cardOf(canonical);
      if (card) {
        const existing = listing.instances.find((i) => perlandOf(i) === canonical) ?? null;
        if (cmd === 'join_preview') return cardPreview(card, existing?.id ?? null);
        if (existing) return existing;
        const taken = new Set(listing.instances.map((i) => i.id));
        const slug = card.name.toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '').replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
        let id = slug;
        for (let n = 2; taken.has(id); n++) id = `${slug}-${n}`;
        const created = instanceFromCard(card, id, canonical);
        listing.instances = sorted([...listing.instances, created]);
        return created;
      }
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
    case 'server_link': {
      const target = listing.instances.find((i) => i.id === args.id) ?? fail('disk', 'instance_not_found', 'gone');
      if (target.source.kind !== 'none') fail('pack', 'link_has_pack', 'This instance already gets its mods from a modpack');
      const canonical = inviteCanonical(String(args.link ?? '')) ?? fail('pack', 'invite_link_invalid', "That isn't a Perland invite link");
      if (!servers || servers.scenario === 'gone') fail('pack', 'invite_gone', "This invite link doesn't work anymore.");
      if (servers.scenario === 'network' || servers.scenario === 'modded-no-pack') {
        fail('pack', 'invite_unsupported', "This server can't be joined from the launcher yet.", { address: 'play.example.com:25577' });
      }
      const linked: Instance = {
        ...target,
        source: { kind: 'perland', invite: canonical, engine: 'https://api.example.com/' },
        updatePolicy: 'auto',
        server: (clone(serverInstanceFx) as Instance).server,
      };
      listing.instances = listing.instances.map((i) => (i.id === linked.id ? linked : i));
      return linked;
    }
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
