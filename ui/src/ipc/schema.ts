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
export const javaSettings = z.strictObject({ path: z.string().nullable(), memoryMb: z.number().int() });
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
export const settings = z.strictObject({
  version: z.number().int(),
  language: lang.nullable(),
  defaultMemoryMb: z.number().int(),
  minimizeOnPlay: z.boolean(),
  firstRunDone: z.boolean(),
});
export const appInfo = z.strictObject({
  version: z.string(),
  build: z.enum(['dev', 'release']),
  offlineAllowed: z.boolean(),
  systemMemoryMb: z.number().int(),
  dataDir: z.string(),
});

export const mcVersion = z.strictObject({
  id: z.string(),
  kind: z.enum(['release', 'snapshot', 'old_beta', 'old_alpha']),
  releaseTime: z.string(),
});
export const loaderVersion = z.strictObject({ version: z.string(), stable: z.boolean() });
export const localMod = z.strictObject({ fileName: z.string(), enabled: z.boolean(), size: z.number().int() });
export const newInstance = z.strictObject({ name: z.string(), minecraft: z.string(), loader: loaderSpec.nullable() });
export const openTarget = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('instance'), id: z.string() }),
  z.strictObject({ kind: z.literal('mods'), id: z.string() }),
  z.strictObject({ kind: z.literal('data') }),
  z.strictObject({ kind: z.literal('logs') }),
  z.strictObject({ kind: z.literal('downloads') }),
]);

export const runStateKind = z.enum(['idle', 'checking', 'asking', 'queued', 'syncing', 'preparing', 'running', 'stopping']);
export const crashHint = z.enum(['out_of_memory']);
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
});
export const exitInfo = z.strictObject({
  code: z.number().int().nullable(),
  killed: z.boolean(),
  cancelled: z.boolean(),
  crash: crashHint.nullable(),
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
export const packArtData = z.strictObject({ icon: z.string().nullable(), banner: z.string().nullable() });
export const packAddRequest = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('link'), url: z.string() }),
  z.strictObject({ kind: z.literal('file'), path: z.string() }),
]);
export const javaTest = z.strictObject({ major: z.number().int() });

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
export type JoinPreview = z.infer<typeof joinPreview>;
export type ServerStatusView = z.infer<typeof serverStatusView>;
export type DeepLink = z.infer<typeof deepLink>;
export type Instance = z.infer<typeof instance>;
export type DamagedInstance = z.infer<typeof damagedInstance>;
export type InstanceListing = z.infer<typeof instanceListing>;
export type Account = z.infer<typeof account>;
export type AccountsFile = z.infer<typeof accountsFile>;
export type Lang = z.infer<typeof lang>;
export type Settings = z.infer<typeof settings>;
export type AppInfo = z.infer<typeof appInfo>;
export type McVersion = z.infer<typeof mcVersion>;
export type LoaderVersion = z.infer<typeof loaderVersion>;
export type LocalMod = z.infer<typeof localMod>;
export type NewInstance = z.infer<typeof newInstance>;
export type OpenTarget = z.infer<typeof openTarget>;
export type RunStateKind = z.infer<typeof runStateKind>;
export type CrashHint = z.infer<typeof crashHint>;
export type Hashes = z.infer<typeof hashes>;
export type ManualFile = z.infer<typeof manualFile>;
export type PackUpdate = z.infer<typeof packUpdate>;
export type SyncSummary = z.infer<typeof syncSummary>;
export type ExitInfo = z.infer<typeof exitInfo>;
export type TaskId = z.infer<typeof taskId>;
export type AppEvent = z.infer<typeof appEvent>;
export type RunSnapshot = z.infer<typeof runSnapshot>;
export type JavaTest = z.infer<typeof javaTest>;
export type PackStatus = z.infer<typeof packStatus>;
export type OptionalChoice = z.infer<typeof optionalChoice>;
export type PackMod = z.infer<typeof packMod>;
export type PackContents = z.infer<typeof packContents>;
export type ManualProgress = z.infer<typeof manualProgress>;
export type PackArtData = z.infer<typeof packArtData>;
export type PackAddRequest = z.infer<typeof packAddRequest>;
