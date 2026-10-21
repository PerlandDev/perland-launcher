// Mirrors of the Rust types the UI receives. Strict objects, so a field added or renamed in Rust
// fails the contract test (ui/src/ipc/contract.test.ts) instead of being silently ignored.
import { z } from 'zod';

export const errorCategory = z.enum(['network', 'auth', 'integrity', 'disk', 'java', 'loader', 'pack', 'game']);
export const launcherError = z.strictObject({
  category: errorCategory,
  code: z.string(),
  message: z.string(),
  fix: z.string().nullable(),
  detail: z.string().nullable(),
  params: z.record(z.string(), z.string()),
});

export const updateState = z.discriminatedUnion('state', [
  z.strictObject({ state: z.literal('off') }),
  z.strictObject({ state: z.literal('idle') }),
  z.strictObject({ state: z.literal('checking') }),
  z.strictObject({ state: z.literal('downloading'), percent: z.number().int().nullable() }),
  z.strictObject({ state: z.literal('ready'), version: z.string() }),
  z.strictObject({ state: z.literal('up_to_date') }),
  z.strictObject({ state: z.literal('failed'), error: launcherError }),
]);
export type UpdateState = z.infer<typeof updateState>;

export const loaderKind = z.enum(['fabric', 'quilt', 'forge', 'neoforge']);
export const loaderSpec = z.strictObject({ kind: loaderKind, version: z.string() });
export const gameSpec = z.strictObject({ minecraft: z.string(), loader: loaderSpec.nullable() });
export const sourceRef = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('none') }),
  z.strictObject({ kind: z.literal('packwiz'), url: z.string() }),
  z.strictObject({ kind: z.literal('mrpack'), name: z.string(), sha1: z.string() }),
  z.strictObject({ kind: z.literal('modrinth'), projectId: z.string(), versionId: z.string() }),
  z.strictObject({ kind: z.literal('perland'), invite: z.string(), engine: z.string() }),
]);
export const javaSettings = z.strictObject({ path: z.string().nullable(), memoryMb: z.number().int(), args: z.string() });
export const serverStatus = z.enum(['online', 'starting', 'sleeping', 'offline']);
export const serverAddress = z.strictObject({ host: z.string(), port: z.number().int() });
export const serverLink = z.strictObject({
  address: serverAddress.nullable(),
  name: z.string(),
  accent: z.string().nullable(),
  whitelisted: z.boolean(),
  lastStatus: serverStatus.nullable(),
  lastSeen: z.number().int().nullable(),
  gone: z.boolean(),
});
export const serverPlayers = z.strictObject({ online: z.number().int(), max: z.number().int().nullable() });
export const serverGame = z.strictObject({
  version: z.string().nullable(),
  loader: z.string().nullable(),
  loaderVersion: z.string().nullable(),
});
export const serverPack = z.strictObject({
  name: z.string(),
  version: z.string().nullable(),
  modCount: z.number().int(),
  state: z.string(),
});
export const serverTeam = z.strictObject({ displayName: z.string(), accent: z.string().nullable() });
export const joinPreview = z.strictObject({
  host: z.string(),
  name: z.string(),
  motd: z.string(),
  kind: z.string(),
  status: serverStatus.nullable(),
  players: serverPlayers.nullable(),
  game: serverGame,
  pack: serverPack.nullable(),
  whitelisted: z.boolean(),
  team: serverTeam.nullable(),
  address: serverAddress.nullable(),
  icon: z.string().nullable(),
  existing: z.string().nullable(),
});
export const listSort = z.enum(['players', 'newest', 'name']);
/** The public server list's query. `sort` and `hasRoom` always travel; every filter is optional. */
export const listQuery = z.strictObject({
  sort: listSort,
  modded: z.boolean().nullable(),
  minecraft: z.string().nullable(),
  hasRoom: z.boolean(),
  category: z.string().nullable(),
  language: z.string().nullable(),
  region: z.string().nullable(),
  offset: z.number().int(),
});
export const publicServerCard = z.strictObject({
  token: z.string(),
  invite: z.string().nullable(),
  joinable: z.boolean(),
  address: z.string().nullable(),
  name: z.string(),
  iconVersion: z.string().nullable(),
  game: serverGame,
  modded: z.boolean(),
  status: serverStatus,
  players: serverPlayers.nullable(),
  whitelisted: z.boolean(),
  pack: serverPack.nullable(),
  categories: z.array(z.string()),
  language: z.string(),
  region: z.string().nullable(),
  instance: z.string().nullable(),
});
export const serverFacets = z.strictObject({
  categories: z.array(z.string()),
  languages: z.array(z.string()),
  minecraftVersions: z.array(z.string()),
  regions: z.array(z.string()),
});
export const publicServerPage = z.strictObject({
  servers: z.array(publicServerCard),
  total: z.number().int(),
  facets: serverFacets,
});
/** Where a Your servers entry comes from. */
export const yourServerRelation = z.enum(['owner', 'invited', 'followed']);
/** A Your servers entry: the public list's card (offline stays `offline` here) plus how the player relates to it. */
export const yourServerCard = publicServerCard.extend({ relation: yourServerRelation, following: z.boolean() });
export const yourServersPage = z.strictObject({ servers: z.array(yourServerCard) });
/** The signed-in player. `email` is null for a Discord-only account; `newPlayer` is true on the sign-in that made it. */
export const perlandAccount = z.strictObject({
  name: z.string(),
  /** The name the player picked, never the email; '' without one. Prefills a free server's name. */
  displayName: z.string(),
  email: z.string().nullable(),
  discord: z.boolean(),
  newPlayer: z.boolean(),
  /** The linked Minecraft name, or null while no Minecraft account is linked. */
  minecraft: z.string().nullable(),
});
/** `enabled` is false while the Engine has player accounts closed; `methods` are the ways to sign in it offers. */
export const perlandStatus = z.strictObject({
  enabled: z.boolean(),
  methods: z.array(z.string()),
  account: perlandAccount.nullable(),
});
/** What every free server has (the Engine's limits; 0 `sleepMinutes` means idle servers never stop). */
export const freeLimits = z.strictObject({
  maxPlayers: z.number().int(),
  memoryMb: z.strictObject({ vanilla: z.number().int(), modded: z.number().int() }),
  diskBytes: z.number(),
  sleepMinutes: z.number().int(),
  unusedDays: z.number().int(),
  warningDays: z.number().int(),
  createsPerWeek: z.number().int(),
});
/** Whether the player may create a free server now. `reason` is the Engine's word (`email_required`, `free_server_exists`,
 *  `too_many_free_servers` with `retryAfter` seconds, `free_servers_full`, `host_full`, `free_servers_paused`, `free_server_blocked`)
 *  and is null while `allowed`. */
export const createAllowance = z.strictObject({
  allowed: z.boolean(),
  reason: z.string().nullable(),
  retryAfter: z.number().int().nullable(),
});
export const freeDeletion = z.strictObject({ deletesAt: z.string(), warned: z.boolean() });
export const freeDisk = z.strictObject({
  usedBytes: z.number(),
  limitBytes: z.number(),
  checkedAt: z.string().nullable(),
  over: z.boolean(),
});
/** The kinds of free server a player can create. */
export const freeServerKind = z.enum(['vanilla', 'modrinth']);
/** The Engine's words for a free server's state. The card's `status` folds `creating`/`deleting` into `sleeping` and `stopped`
 *  into `offline`, so take the words from `freeStatus`. A newer Engine may add one: `freeServerCard.freeStatus` stays a string. */
export const freeStatus = z.enum(['online', 'starting', 'sleeping', 'offline', 'stopped', 'creating', 'deleting']);
/** The owner's free server as a card: the public list's card fields plus the free-server extras. */
export const freeServerCard = publicServerCard.extend({
  kind: z.string(),
  memoryMb: z.number().int(),
  maxPlayers: z.number().int(),
  freeStatus: z.string(),
  deletion: freeDeletion.nullable(),
  disk: freeDisk,
  /** The invite link to share. It holds the invite's credential: never log it. */
  inviteUrl: z.string().nullable(),
  /** `perland://join?invite=…`, for friends who have the launcher. */
  joinUrl: z.string().nullable(),
});
/** `free_server`: null while signed out or when free servers are off. */
export const freeServerPage = z.strictObject({
  limits: freeLimits,
  create: createAllowance,
  server: freeServerCard.nullable(),
});
/** The code to type in game (`/perland link`), and the seconds it stays good. */
export const linkCode = z.strictObject({ code: z.string(), expiresIn: z.number() });
/** One server's numbers for the linked player; `instance` is the launcher instance that joins it, if there is one. */
export const serverStats = z.strictObject({
  token: z.string(),
  instance: z.string().nullable(),
  name: z.string(),
  playtimeSeconds: z.number(),
  advancements: z.number(),
  /** The completed advancements' IDs (`minecraft:story/mine_stone`), sorted. */
  advancementIds: z.array(z.string()),
});
export const perlandStats = z.strictObject({ servers: z.array(serverStats) });
export const emailCodeSent = z.strictObject({ resendIn: z.number().int() });
const newsNameList = z.strictObject({ count: z.number().int(), names: z.array(z.string()) });
export const newsChangesView = z.strictObject({ added: newsNameList, removed: newsNameList, updated: newsNameList });
export const newsPostView = z.strictObject({
  id: z.string(),
  type: z.enum(['update', 'event', 'modpack']),
  title: z.string(),
  /** Plain text: render it as text, never as HTML or Markdown. */
  body: z.string(),
  startsAt: z.string().nullable(),
  endsAt: z.string().nullable(),
  imageVersion: z.string().nullable(),
  modpack: newsChangesView.nullable(),
  createdAt: z.string(),
});
export const serverNews = z.strictObject({
  posts: z.array(newsPostView),
  nextBefore: z.string().nullable(),
  seen: z.string().nullable(),
});
export const serverStatusView = z.strictObject({
  status: serverStatus.nullable(),
  players: serverPlayers.nullable(),
  whitelisted: z.boolean(),
  packState: z.string().nullable(),
  gone: z.boolean(),
  address: serverAddress.nullable(),
});
export const deepLink = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('pack'), url: z.string() }),
  z.strictObject({ kind: z.literal('join'), invite: z.string() }),
]);
export const inviteLinkCase = z.strictObject({ input: z.string(), canonical: z.string().nullable() });
export const instance = z.strictObject({
  id: z.string(),
  name: z.string(),
  source: sourceRef,
  game: gameSpec,
  updatePolicy: z.enum(['auto', 'ask', 'pinned']),
  optionalChoices: z.record(z.string(), z.boolean()),
  java: javaSettings,
  server: serverLink.nullable(),
  artSeed: z.string().nullable(),
  art: z.enum(['auto', 'generated', 'custom']),
  icon: z.enum(['auto', 'generated', 'custom']).nullable(),
  /** Which point of a picture background stays in view, in percent of its width and height; null is the default (bottom middle). */
  bannerFocus: z.strictObject({ x: z.number().int(), y: z.number().int() }).nullable(),
});
export const damagedInstance = z.strictObject({ id: z.string(), error: launcherError });
export const instanceListing = z.strictObject({ instances: z.array(instance), damaged: z.array(damagedInstance) });

export const account = z.strictObject({ provider: z.enum(['microsoft', 'offline']), username: z.string(), uuid: z.string() });
export const accountsFile = z.strictObject({
  accounts: z.array(account),
  active: z.string().nullable(),
  licensedSigninSeen: z.boolean(),
});

export const lang = z.enum(['en', 'es']);
export const channel = z.enum(['stable', 'beta']);
/** A theme swaps the neutrals only; the first key is the default. */
export const theme = z.enum(['laminate', 'stone', 'night', 'forest']);
export const uiFont = z.enum(['hyperlegible', 'system']);
export const settings = z.strictObject({
  version: z.number().int(),
  language: lang.nullable(),
  defaultMemoryMb: z.number().int(),
  minimizeOnPlay: z.boolean(),
  firstRunDone: z.boolean(),
  updateChannel: channel.nullable(),
  railExpanded: z.boolean(),
  iconShape: z.enum(['square', 'round']),
  theme,
  font: uiFont,
});
export const appInfo = z.strictObject({
  version: z.string(),
  build: z.enum(['dev', 'beta', 'release']),
  offlineAllowed: z.boolean(),
  systemMemoryMb: z.number().int(),
  dataDir: z.string(),
  /** A Perland panel is configured: the servers the player owns offer Manage in Perland. */
  panel: z.boolean(),
});

export const mcVersion = z.strictObject({
  id: z.string(),
  kind: z.enum(['release', 'snapshot', 'old_beta', 'old_alpha']),
  releaseTime: z.string(),
});
export const loaderVersion = z.strictObject({ version: z.string(), stable: z.boolean() });
export const localMod = z.strictObject({ fileName: z.string(), enabled: z.boolean(), size: z.number().int() });
export const contentFile = z.strictObject({
  fileName: z.string(),
  size: z.number().int(),
  folder: z.boolean(),
  managed: z.boolean(),
});
export const newInstance = z.strictObject({ name: z.string(), minecraft: z.string(), loader: loaderSpec.nullable() });
export const shareCategory = z.enum(['mods', 'settings', 'resourcePacks', 'shaderPacks', 'worlds']);

/** One Share choice's numbers: files (world folders for worlds), how many link to Modrinth, and the bytes the file carries. */
export const categoryCount = z.strictObject({
  category: shareCategory,
  files: z.number().int(),
  linked: z.number().int(),
  embeddedBytes: z.number().int(),
});

export const sharePreview = z.strictObject({
  name: z.string(),
  fileName: z.string(),
  counts: categoryCount.array(),
});

export const shareChoice = z.strictObject({
  mods: z.boolean(),
  settings: z.boolean(),
  resourcePacks: z.boolean(),
  shaderPacks: z.boolean(),
  worlds: z.boolean(),
});

export const importPreview = z.strictObject({
  path: z.string(),
  name: z.string().nullable(),
  minecraft: z.string().nullable(),
  loader: loaderSpec.nullable(),
  sizeBytes: z.number().int(),
  memoryMb: z.number().int().nullable(),
  memoryWantedMb: z.number().int().nullable(),
  javaArgs: z.string().nullable(),
  playtimeSeconds: z.number().int().nullable(),
});
export const importRequest = z.strictObject({ path: z.string(), name: z.string(), minecraft: z.string(), loader: loaderSpec.nullable() });
export const openTarget = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('instance'), id: z.string() }),
  z.strictObject({ kind: z.literal('mods'), id: z.string() }),
  z.strictObject({ kind: z.literal('resource_packs'), id: z.string() }),
  z.strictObject({ kind: z.literal('shader_packs'), id: z.string() }),
  z.strictObject({ kind: z.literal('data') }),
  z.strictObject({ kind: z.literal('logs') }),
  z.strictObject({ kind: z.literal('downloads') }),
]);

export const runStateKind = z.enum(['idle', 'checking', 'asking', 'queued', 'syncing', 'preparing', 'waking', 'running', 'stopping']);
export const crashFix = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('more_memory') }),
  z.strictObject({ kind: z.literal('recommended_java') }),
  z.strictObject({ kind: z.literal('java_settings') }),
  z.strictObject({ kind: z.literal('find_mod'), project: z.string() }),
  z.strictObject({ kind: z.literal('disable_mod'), file: z.string(), name: z.string() }),
  z.strictObject({ kind: z.literal('repair') }),
  z.strictObject({ kind: z.literal('update_drivers'), vendor: z.enum(['amd', 'nvidia', 'intel']) }),
]);
export const hashes = z.strictObject({
  sha1: z.string().nullable(),
  sha256: z.string().nullable(),
  sha512: z.string().nullable(),
  md5: z.string().nullable(),
  murmur2: z.number().int().nullable(),
});
export const manualFile = z.strictObject({
  path: z.string(),
  name: z.string(),
  fileName: z.string(),
  pageUrl: z.string(),
  hashes,
});
export const packUpdate = z.strictObject({
  fromVersion: z.string().nullable(),
  toVersion: z.string(),
  added: z.array(z.string()),
  updated: z.array(z.string()),
  removed: z.array(z.string()),
  kept: z.array(z.string()),
});
export const syncSummary = z.strictObject({
  fromVersion: z.string().nullable(),
  toVersion: z.string(),
  added: z.number().int(),
  updated: z.number().int(),
  removed: z.number().int(),
  keptModified: z.array(z.string()),
  setAside: z.array(z.string()),
});
export const exitInfo = z.strictObject({
  code: z.number().int().nullable(),
  killed: z.boolean(),
  cancelled: z.boolean(),
  cause: launcherError.nullable(),
  fixes: z.array(crashFix),
  error: launcherError.nullable(),
  synced: syncSummary.nullable(),
  manual: z.array(manualFile),
  updateOnly: z.boolean(),
});
export const taskId = z.strictObject({ key: z.string(), params: z.record(z.string(), z.string()) });
export const appEvent = z.discriminatedUnion('type', [
  z.strictObject({
    type: z.literal('progress'),
    instance: z.string().nullable(),
    task: taskId,
    done: z.number(),
    total: z.number(),
    bytes: z.number(),
  }),
  z.strictObject({ type: z.literal('log'), instance: z.string(), lines: z.array(z.string()) }),
  z.strictObject({
    type: z.literal('run_state'),
    instance: z.string(),
    state: runStateKind,
    exit: exitInfo.nullable(),
    update: packUpdate.nullable(),
  }),
  z.strictObject({ type: z.literal('warning'), instance: z.string().nullable(), error: launcherError }),
  z.strictObject({ type: z.literal('deep_link') }),
  z.strictObject({ type: z.literal('update'), state: updateState }),
  z.strictObject({ type: z.literal('perland_sign_in'), phase: z.enum(['waiting', 'exchanging']) }),
]);
export const runSnapshot = z.strictObject({ instance: z.string(), state: runStateKind, update: packUpdate.nullable() });
export const packStatus = z.strictObject({
  installedVersion: z.string().nullable(),
  update: packUpdate.nullable(),
  checkError: launcherError.nullable(),
});
export const optionalChoice = z.strictObject({ enabled: z.boolean(), description: z.string().nullable() });
export const packMod = z.strictObject({ path: z.string(), name: z.string(), optional: optionalChoice.nullable() });
export const packContents = z.strictObject({ managed: z.array(packMod), additions: z.array(localMod) });
export const manualProgress = z.strictObject({ found: z.number().int(), total: z.number().int(), remaining: z.array(manualFile) });
export const instanceArtData = z.strictObject({ icon: z.string().nullable(), banner: z.string().nullable() });
export const artChoice = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('generated'), seed: z.string() }),
  z.strictObject({ kind: z.literal('pack') }),
  z.strictObject({ kind: z.literal('upload'), path: z.string() }),
  z.strictObject({ kind: z.literal('screenshot'), name: z.string() }),
]);
export const iconChoice = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('generated') }),
  z.strictObject({ kind: z.literal('pack') }),
  z.strictObject({ kind: z.literal('upload'), path: z.string() }),
]);
export const packAddRequest = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('link'), url: z.string() }),
  z.strictObject({ kind: z.literal('file'), path: z.string() }),
]);
export const javaFit = z.enum(['matches', 'too_old', 'newer']);
export const javaFound = z.strictObject({
  path: z.string(),
  major: z.number().int(),
  version: z.string(),
  vendor: z.string().nullable(),
  fit: javaFit.nullable(),
  recommended: z.boolean(),
});
export const javaOverview = z.strictObject({ required: z.number().int().nullable(), installs: z.array(javaFound) });
export const playtimeEntry = z.strictObject({ id: z.string(), seconds: z.number().int(), lastPlayed: z.number().int() });
/** Where Home's Continue takes the player: a world (its folder) or a server (its name). */
export const continueTarget = z.strictObject({ kind: z.enum(['world', 'server']), name: z.string() });

export const contentKind = z.enum(['modpack', 'mod', 'resource_pack', 'shader']);
export const sortIndex = z.enum(['relevance', 'downloads', 'follows', 'newest', 'updated']);
export const imageKind = z.enum(['icon', 'gallery']);
export const searchHit = z.strictObject({
  projectId: z.string(),
  slug: z.string(),
  title: z.string(),
  author: z.string(),
  summary: z.string(),
  iconUrl: z.string().nullable(),
  downloads: z.number().int(),
  follows: z.number().int(),
  categories: z.array(z.string()),
  kind: contentKind,
});
export const searchPage = z.strictObject({ hits: z.array(searchHit), total: z.number().int(), offset: z.number().int() });
export const sideSupport = z.enum(['required', 'optional', 'unsupported', 'unknown']);
export const galleryImage = z.strictObject({ url: z.string(), title: z.string().nullable(), featured: z.boolean() });
export const projectDetail = z.strictObject({
  projectId: z.string(),
  slug: z.string(),
  title: z.string(),
  author: z.string().nullable(),
  summary: z.string(),
  body: z.string(),
  iconUrl: z.string().nullable(),
  downloads: z.number().int(),
  follows: z.number().int(),
  categories: z.array(z.string()),
  kind: contentKind,
  clientSide: sideSupport,
  serverSide: sideSupport,
  license: z.string().nullable(),
  sourceUrl: z.string().nullable(),
  issuesUrl: z.string().nullable(),
  wikiUrl: z.string().nullable(),
  gallery: z.array(galleryImage),
});
export const versionInfo = z.strictObject({
  id: z.string(),
  name: z.string(),
  number: z.string(),
  versionType: z.string(),
  gameVersions: z.array(z.string()),
  loaders: z.array(z.string()),
  date: z.string(),
  size: z.number().int().nullable(),
});
export const itemReason = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('requested') }),
  z.strictObject({ kind: z.literal('dependency'), of: z.string() }),
  z.strictObject({ kind: z.literal('shader_mod') }),
]);
export const planSkip = z.strictObject({ projectId: z.string(), title: z.string(), reason: z.enum(['installed', 'in_pack']) });
export const planItemView = z.strictObject({
  projectId: z.string(),
  title: z.string(),
  kind: contentKind,
  versionNumber: z.string(),
  fileName: z.string(),
  size: z.number().int(),
  reason: itemReason,
});
export const planTargetView = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('instance'), id: z.string(), name: z.string(), minecraft: z.string(), loader: loaderKind.nullable() }),
  z.strictObject({ kind: z.literal('new'), name: z.string(), minecraft: z.string(), loader: loaderKind.nullable() }),
]);
export const installPlanView = z.strictObject({
  planId: z.string(),
  target: planTargetView,
  items: z.array(planItemView),
  skipped: z.array(planSkip),
  totalSize: z.number().int(),
});
export const installResult = z.strictObject({ instance, added: z.array(z.string()) });
export const searchRequest = z.strictObject({
  query: z.string(),
  kind: contentKind,
  instance: z.string().nullable(),
  minecraft: z.string().nullable(),
  loader: loaderKind.nullable(),
  categories: z.array(z.string()),
  sort: sortIndex,
  offset: z.number().int(),
});
export const browsePlanRequest = z.strictObject({
  project: z.string(),
  version: z.string().nullable(),
  target: z.discriminatedUnion('kind', [
    z.strictObject({ kind: z.literal('instance'), id: z.string() }),
    z.strictObject({ kind: z.literal('new') }),
  ]),
});

export type ErrorCategory = z.infer<typeof errorCategory>;
export type LauncherError = z.infer<typeof launcherError>;
export type LoaderKind = z.infer<typeof loaderKind>;
export type LoaderSpec = z.infer<typeof loaderSpec>;
export type GameSpec = z.infer<typeof gameSpec>;
export type SourceRef = z.infer<typeof sourceRef>;
export type JavaSettings = z.infer<typeof javaSettings>;
export type ServerStatus = z.infer<typeof serverStatus>;
export type ServerAddress = z.infer<typeof serverAddress>;
export type ServerLink = z.infer<typeof serverLink>;
export type ServerPlayers = z.infer<typeof serverPlayers>;
export type ListSort = z.infer<typeof listSort>;
export type ListQuery = z.infer<typeof listQuery>;
export type PublicServerCard = z.infer<typeof publicServerCard>;
export type ServerFacets = z.infer<typeof serverFacets>;
export type PublicServerPage = z.infer<typeof publicServerPage>;
export type YourServerRelation = z.infer<typeof yourServerRelation>;
export type YourServerCard = z.infer<typeof yourServerCard>;
export type YourServersPage = z.infer<typeof yourServersPage>;
export type FreeLimits = z.infer<typeof freeLimits>;
export type CreateAllowance = z.infer<typeof createAllowance>;
export type FreeDeletion = z.infer<typeof freeDeletion>;
export type FreeDisk = z.infer<typeof freeDisk>;
export type FreeServerKind = z.infer<typeof freeServerKind>;
export type FreeStatus = z.infer<typeof freeStatus>;
export type FreeServerCard = z.infer<typeof freeServerCard>;
export type FreeServerPage = z.infer<typeof freeServerPage>;
export type PerlandAccount = z.infer<typeof perlandAccount>;
export type PerlandStatus = z.infer<typeof perlandStatus>;
export type LinkCode = z.infer<typeof linkCode>;
export type ServerStats = z.infer<typeof serverStats>;
export type PerlandStats = z.infer<typeof perlandStats>;
export type EmailCodeSent = z.infer<typeof emailCodeSent>;
export type NewsChangesView = z.infer<typeof newsChangesView>;
export type NewsPostView = z.infer<typeof newsPostView>;
export type ServerNews = z.infer<typeof serverNews>;
export type JoinPreview = z.infer<typeof joinPreview>;
export type ServerStatusView = z.infer<typeof serverStatusView>;
export type DeepLink = z.infer<typeof deepLink>;
export type Instance = z.infer<typeof instance>;
export type Focus = NonNullable<Instance['bannerFocus']>;
export type DamagedInstance = z.infer<typeof damagedInstance>;
export type InstanceListing = z.infer<typeof instanceListing>;
export type Account = z.infer<typeof account>;
export type AccountsFile = z.infer<typeof accountsFile>;
export type Lang = z.infer<typeof lang>;
export type Channel = z.infer<typeof channel>;
export type Theme = z.infer<typeof theme>;
export type UiFont = z.infer<typeof uiFont>;
export type Settings = z.infer<typeof settings>;
export type AppInfo = z.infer<typeof appInfo>;
export type McVersion = z.infer<typeof mcVersion>;
export type LoaderVersion = z.infer<typeof loaderVersion>;
export type LocalMod = z.infer<typeof localMod>;
export type ContentFile = z.infer<typeof contentFile>;
export type NewInstance = z.infer<typeof newInstance>;
export type ImportPreview = z.infer<typeof importPreview>;
export type ShareCategory = z.infer<typeof shareCategory>;
export type CategoryCount = z.infer<typeof categoryCount>;
export type SharePreview = z.infer<typeof sharePreview>;
export type ShareChoice = z.infer<typeof shareChoice>;
export type ImportRequest = z.infer<typeof importRequest>;
export type OpenTarget = z.infer<typeof openTarget>;
export type RunStateKind = z.infer<typeof runStateKind>;
export type CrashFix = z.infer<typeof crashFix>;
export type Hashes = z.infer<typeof hashes>;
export type ManualFile = z.infer<typeof manualFile>;
export type PackUpdate = z.infer<typeof packUpdate>;
export type SyncSummary = z.infer<typeof syncSummary>;
export type ExitInfo = z.infer<typeof exitInfo>;
export type TaskId = z.infer<typeof taskId>;
export type AppEvent = z.infer<typeof appEvent>;
export type RunSnapshot = z.infer<typeof runSnapshot>;
export type JavaFound = z.infer<typeof javaFound>;
export type JavaOverview = z.infer<typeof javaOverview>;
export type PlaytimeEntry = z.infer<typeof playtimeEntry>;
export type ContinueTarget = z.infer<typeof continueTarget>;
export type PackStatus = z.infer<typeof packStatus>;
export type OptionalChoice = z.infer<typeof optionalChoice>;
export type PackMod = z.infer<typeof packMod>;
export type PackContents = z.infer<typeof packContents>;
export type ManualProgress = z.infer<typeof manualProgress>;
export type InstanceArtData = z.infer<typeof instanceArtData>;
export type ArtChoice = z.infer<typeof artChoice>;
export type IconChoice = z.infer<typeof iconChoice>;
export type PackAddRequest = z.infer<typeof packAddRequest>;
export type ContentKind = z.infer<typeof contentKind>;
export type SortIndex = z.infer<typeof sortIndex>;
export type ImageKind = z.infer<typeof imageKind>;
export type SearchHit = z.infer<typeof searchHit>;
export type SearchPage = z.infer<typeof searchPage>;
export type SideSupport = z.infer<typeof sideSupport>;
export type GalleryImage = z.infer<typeof galleryImage>;
export type ProjectDetail = z.infer<typeof projectDetail>;
export type VersionInfo = z.infer<typeof versionInfo>;
export type ItemReason = z.infer<typeof itemReason>;
export type PlanSkip = z.infer<typeof planSkip>;
export type PlanItemView = z.infer<typeof planItemView>;
export type PlanTargetView = z.infer<typeof planTargetView>;
export type InstallPlanView = z.infer<typeof installPlanView>;
export type InstallResult = z.infer<typeof installResult>;
export type SearchRequest = z.infer<typeof searchRequest>;
export type BrowsePlanRequest = z.infer<typeof browsePlanRequest>;
