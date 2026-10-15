// Browser-only backend for `pnpm --filter ui dev:mock` and tests: seeded from the Rust contract
// fixtures, with a scripted Play. main.tsx only imports it in mock mode, so the Tauri build
// never runs it.
import { emit } from '@tauri-apps/api/event';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import accountsFx from './contract/accounts.json';
import appInfoFx from './contract/app_info.json';
import listingFx from './contract/instance_listing.json';
import loaderVersionsFx from './contract/loader_versions.json';
import localModsFx from './contract/local_mods.json';
import manualProgressFx from './contract/manual_progress.json';
import mcVersionsFx from './contract/mc_versions.json';
import packContentsFx from './contract/pack_contents.json';
import packStatusFx from './contract/pack_status.json';
import projectDetailFx from './contract/project_detail.json';
import settingsFx from './contract/settings.json';
import versionsFx from './contract/versions.json';
import { fitFor, fitsVersion, pickDefault } from '../features/browse/compat';
import { APP_EVENT } from './events';
import { inviteCanonical } from './inviteLink';
import { handleServer, MOCK_SERVER_ID, NOT_MINE, seedServers, type ServerScenario } from './mockServers';
import type {
  AccountsFile,
  AppEvent,
  AppInfo,
  BrowsePlanRequest,
  ContentFile,
  ContentKind,
  DeepLink,
  ExitInfo,
  InstallPlanView,
  InstallResult,
  Instance,
  InstanceListing,
  LauncherError,
  LocalMod,
  ManualFile,
  ManualProgress,
  NewInstance,
  PackAddRequest,
  PackArtData,
  PackContents,
  PackStatus,
  PackUpdate,
  PlanItemView,
  PlanSkip,
  ProjectDetail,
  RunStateKind,
  SearchHit,
  SearchPage,
  SearchRequest,
  Settings,
  SyncSummary,
  UpdateState,
  VersionInfo,
} from './schema';

export interface MockOptions {
  firstRun?: boolean;
  crash?: boolean;
  release?: boolean;
  /** An optimized beta build: offline accounts stay open, About says so. */
  beta?: boolean;
  /** The launcher's own update state; defaults to idle on release and beta builds, off in dev. */
  update?: UpdateState;
  /** The pack's first sync ends with two files the player has to download by hand. */
  manual?: boolean;
  /** The pack has no update on offer. */
  noUpdate?: boolean;
  /** A pending perland:// or pack link, handed over once by deep_link_take. */
  link?: string;
  /** Seeds the `survival` Perland server and picks how its invite answers. */
  server?: ServerScenario;
  /** What the save dialog returns; null means the player cancelled. Defaults to a zip in Downloads. */
  savePath?: string | null;
}

interface MockRun {
  state: RunStateKind;
  cancelled: boolean;
  /** The banner's Update: sync only, no game. */
  updateOnly: boolean;
  /** Set while asking: resolves with the player's answer. */
  answer: ((update: boolean) => void) | null;
}

interface MockPack {
  installed: string | null;
  update: PackUpdate | null;
  contents: PackContents;
  art: PackArtData;
  manual: ManualFile[];
  found: number;
}

const svg = (body: string) => `data:image/svg+xml;base64,${btoa(body)}`;
const SKY_ART: PackArtData = {
  icon: svg(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" shape-rendering="crispEdges"><rect width="16" height="16" fill="#7cc4ff"/><rect y="9" width="16" height="7" fill="#3f8f4f"/><rect y="12" width="16" height="4" fill="#6b4a2b"/></svg>',
  ),
  banner: svg(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 270" shape-rendering="crispEdges"><defs><linearGradient id="s" x2="0" y2="1"><stop offset="0" stop-color="#9fd8ff"/><stop offset="1" stop-color="#e9c46a"/></linearGradient></defs><rect width="480" height="270" fill="url(#s)"/><rect x="60" y="120" width="140" height="40" fill="#3f8f4f"/><rect x="80" y="160" width="100" height="24" fill="#6b4a2b"/><rect x="290" y="90" width="120" height="36" fill="#3f8f4f"/><rect x="306" y="126" width="88" height="20" fill="#6b4a2b"/></svg>',
  ),
};

const clone = <T>(value: T): T => structuredClone(value);
const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function fail(category: LauncherError['category'], code: string, message: string, params = {}): never {
  throw { category, code, message, fix: null, detail: null, params } satisfies LauncherError;
}

function slug(name: string) {
  const s = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  return s.slice(0, 48) || 'instance';
}

// A tiny Modrinth for the browser build and the tests. Icons are one inline SVG.
const MOCK_ICON = svg('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8" fill="#c9cc2a"/></svg>');

interface MockProject {
  detail: ProjectDetail;
  versions: VersionInfo[];
  /** Project ids every version requires. */
  deps: string[];
}

function mockProject(
  projectId: string,
  title: string,
  kind: ContentKind,
  loaders: string[],
  deps: string[] = [],
  extra: Partial<ProjectDetail> = {},
): MockProject {
  const base = clone(projectDetailFx) as ProjectDetail;
  const version = (clone(versionsFx) as VersionInfo[])[0]!;
  return {
    detail: { ...base, projectId, slug: title.toLowerCase().replace(/\s+/g, '-'), title, kind, categories: [], gallery: [], ...extra },
    versions: [{ ...version, id: `${projectId}-v1`, name: `${title} 1.0`, number: '1.0', loaders, gameVersions: ['1.20.1', '1.21.1'] }],
    deps,
  };
}

const CATALOG: MockProject[] = [
  { ...mockProject('AANobbMI', 'Sodium', 'mod', ['fabric', 'quilt'], ['P7dR8mSH']), detail: clone(projectDetailFx) as ProjectDetail },
  mockProject('P7dR8mSH', 'Fabric API', 'mod', ['fabric'], [], { categories: ['library'] }),
  mockProject('YL57xq9U', 'Iris', 'mod', ['fabric', 'quilt', 'neoforge'], ['AANobbMI']),
  mockProject('HVnmMxH1', 'Complementary', 'shader', ['iris']),
  mockProject('FAITHFUL', 'Faithful', 'resource_pack', ['minecraft']),
  mockProject('1KVo5zza', 'Fabulously Optimized', 'modpack', ['fabric']),
  ...Array.from({ length: 24 }, (_, i) =>
    mockProject(`FILL${String(i + 1).padStart(4, '0')}`, `Mod ${i + 1}`, 'mod', ['fabric', 'forge']),
  ),
];
// Sodium's fixture versions only list 1.20.1; keep its id matching the catalog's.
CATALOG[0]!.versions = (clone(versionsFx) as VersionInfo[]).map((v) => ({ ...v, id: 'AANobbMI-v1' }));

const findProject = (ref: string) => CATALOG.find((p) => p.detail.projectId === ref || p.detail.slug === ref);

const contentKey = (id: string, kind: ContentKind) => `${id}:${kind}`;

function seed(options: MockOptions) {
  const listing = clone(listingFx) as InstanceListing;
  const accounts = clone(accountsFx) as AccountsFile;
  const settings: Settings = { ...(clone(settingsFx) as Settings), firstRunDone: true, language: null };
  if (options.firstRun) {
    listing.instances = [];
    listing.damaged = [];
    accounts.accounts = [];
    accounts.active = null;
    settings.firstRunDone = false;
  }
  const appInfo: AppInfo = {
    ...(clone(appInfoFx) as AppInfo),
    build: options.release ? 'release' : options.beta ? 'beta' : 'dev',
    offlineAllowed: !options.release,
  };
  const update: UpdateState = options.update ?? { state: options.release || options.beta ? 'idle' : 'off' };
  const mods = new Map<string, LocalMod[]>([['asalith-fields', clone(localModsFx) as LocalMod[]]]);
  const packs = new Map<string, MockPack>();
  // Resource packs and shaders per instance and kind, like the real folders. Mods have their own list.
  const content = new Map<string, ContentFile[]>();
  if (!options.firstRun) {
    content.set(contentKey('asalith-fields', 'resource_pack'), [
      { fileName: 'Faithful 32x.zip', size: 3_000_000, folder: false, managed: false },
      { fileName: 'My Pack', size: 0, folder: true, managed: false },
    ]);
    content.set(contentKey('asalith-fields', 'shader'), [
      { fileName: 'ComplementaryReimagined.zip', size: 1_500_000, folder: false, managed: false },
    ]);
  }
  if (!options.firstRun) {
    const sky: Instance = {
      id: 'sky-islands',
      name: 'Sky Islands',
      source: { kind: 'packwiz', url: 'https://packs.example/sky-islands/pack.toml' },
      game: { minecraft: '1.20.1', loader: { kind: 'forge', version: '47.4.22' } },
      updatePolicy: 'ask',
      optionalChoices: {},
      java: { path: null, memoryMb: 6144 },
      server: null,
    };
    listing.instances = [...listing.instances, sky];
    const offer = (clone(packStatusFx) as PackStatus[])[0]?.update ?? null;
    const blocked = (clone(manualProgressFx) as ManualProgress).remaining;
    content.set(contentKey(sky.id, 'resource_pack'), [{ fileName: 'Pack Textures.zip', size: 2_000_000, folder: false, managed: true }]);
    packs.set(sky.id, {
      installed: '1.0.3',
      update: options.noUpdate ? null : offer,
      contents: clone(packContentsFx) as PackContents,
      art: SKY_ART,
      manual: options.manual
        ? [
            ...blocked,
            {
              ...blocked[0]!,
              path: 'mods/sodium-extra.jar',
              name: 'Sodium Extra',
              fileName: 'sodium-extra-0.5.4.jar',
              pageUrl: 'https://www.curseforge.com/projects/447673',
            },
          ]
        : [],
      found: 0,
    });
  }
  const servers = seedServers(options.server, listing);
  return {
    listing,
    servers,
    accounts,
    settings,
    appInfo,
    update,
    mods,
    packs,
    content,
    browse: {
      installed: new Map<string, Set<string>>([['asalith-fields', new Set(['P7dR8mSH'])]]),
      plans: new Map<string, { view: InstallPlanView; kind: ContentKind }>(),
      nextPlan: 1,
      opened: [] as string[],
    },
    pendingLink: options.link ?? null,
    runs: new Map<string, MockRun>(),
    options,
  };
}

export type MockState = ReturnType<typeof seed>;

function event(e: AppEvent) {
  // Tests that call handle() directly have no IPC mock installed; an event nobody hears is fine.
  emit(APP_EVENT, e).catch(() => {});
}

export const exitOf = (partial: Partial<ExitInfo>): ExitInfo => ({
  code: null,
  killed: false,
  cancelled: false,
  crash: null,
  error: null,
  synced: null,
  manual: [],
  updateOnly: false,
  ...partial,
});

function setState(state: MockState, id: string, next: RunStateKind) {
  const run = state.runs.get(id);
  if (run) run.state = next;
  event({ type: 'run_state', instance: id, state: next, exit: null, update: null });
}

/** Check, ask, sync, like the real run. Returns the sync summary, or null when the run was
 *  stopped or ended early (update only, or blocked files). */
async function packPrelude(
  state: MockState,
  instance: Instance,
  run: MockRun,
  updateOnly: boolean,
): Promise<{ synced: SyncSummary | null } | null> {
  const id = instance.id;
  const pack = state.packs.get(id);
  if (!pack) return { synced: null };
  const checks = pack.installed !== null && instance.updatePolicy !== 'pinned';
  let take = false;
  if (checks) {
    await wait(400);
    if (run.cancelled) return null;
    if (pack.update) {
      if (updateOnly || instance.updatePolicy === 'auto') take = true;
      else {
        run.state = 'asking';
        event({ type: 'run_state', instance: id, state: 'asking', exit: null, update: pack.update });
        take = await new Promise<boolean>((resolve) => (run.answer = resolve));
        if (run.cancelled) return null;
      }
    }
    setState(state, id, 'queued');
  }
  await wait(200);
  if (run.cancelled) return null;
  setState(state, id, 'syncing');
  if (pack.installed) {
    // A pack that is already installed gets its files checked first (hashing takes a while).
    const files = 6;
    for (let done = 0; done <= files; done++) {
      if (run.cancelled) return null;
      event({
        type: 'progress',
        instance: id,
        task: { key: 'check_pack', params: { name: instance.name } },
        done,
        total: files,
        bytes: 0,
      });
      await wait(40);
    }
  }
  const total = 24;
  for (let done = 0; done <= total; done++) {
    if (run.cancelled) return null;
    event({
      type: 'progress',
      instance: id,
      task: { key: pack.installed ? 'sync_pack' : 'install_pack', params: { name: instance.name } },
      done,
      total,
      bytes: done * 2_400_000,
    });
    await wait(50);
  }
  if (run.cancelled) return null;
  if (pack.found < pack.manual.length) {
    state.runs.delete(id);
    const error: LauncherError = {
      category: 'pack',
      code: 'manual_downloads_needed',
      message: `${pack.manual.length} files have to be downloaded by hand`,
      fix: null,
      detail: null,
      params: { count: String(pack.manual.length) },
    };
    event({ type: 'run_state', instance: id, state: 'idle', exit: exitOf({ error, manual: pack.manual, updateOnly }), update: null });
    return null;
  }
  let synced: SyncSummary | null = null;
  if (take && pack.update) {
    const u = pack.update;
    synced = {
      fromVersion: u.fromVersion,
      toVersion: u.toVersion,
      added: u.added.length,
      updated: u.updated.length,
      removed: u.removed.length,
      keptModified: u.kept,
    };
    pack.installed = u.toVersion;
    pack.update = null;
  } else if (!pack.installed) {
    pack.installed = '1.0.3';
  }
  if (updateOnly) {
    state.runs.delete(id);
    event({ type: 'run_state', instance: id, state: 'idle', exit: exitOf({ synced, updateOnly: true }), update: null });
    return null;
  }
  return { synced };
}

async function scriptRun(state: MockState, instance: Instance, updateOnly = false) {
  const id = instance.id;
  const pack = state.packs.get(id);
  const first: RunStateKind = pack && pack.installed !== null && instance.updatePolicy !== 'pinned' ? 'checking' : 'queued';
  const run: MockRun = { state: first, cancelled: false, updateOnly, answer: null };
  state.runs.set(id, run);
  event({ type: 'run_state', instance: id, state: first, exit: null, update: null });
  let synced: SyncSummary | null = null;
  if (pack) {
    const outcome = await packPrelude(state, instance, run, updateOnly);
    if (!outcome || run.cancelled) return;
    synced = outcome.synced;
  } else {
    await wait(300);
    if (run.cancelled) return;
  }
  setState(state, id, 'preparing');
  for (let done = 0; done <= 40; done++) {
    if (run.cancelled) return;
    event({
      type: 'progress',
      instance: id,
      task: { key: 'install_minecraft', params: { version: instance.game.minecraft } },
      done,
      total: 40,
      bytes: done * 7_800_000,
    });
    await wait(40);
  }
  setState(state, id, 'running');
  const started = Date.now();
  let n = 0;
  while (state.runs.get(id) === run && run.state === 'running') {
    event({ type: 'log', instance: id, lines: [`[12:00:${String(n % 60).padStart(2, '0')}] [Render thread/INFO]: Tick ${n}`] });
    n += 1;
    if (state.options.crash && Date.now() - started > 2500) {
      event({ type: 'log', instance: id, lines: ['Exception in thread "main" java.lang.OutOfMemoryError: Java heap space'] });
      state.runs.delete(id);
      event({
        type: 'run_state',
        instance: id,
        state: 'idle',
        exit: exitOf({ code: 1, crash: 'out_of_memory', synced }),
        update: null,
      });
      return;
    }
    await wait(400);
  }
}

/** A server whose pack is still building: Play waits in `checking` with "Server is preparing
 *  the modpack…", then runs like any instance. */
async function scriptServerWait(state: MockState, instance: Instance) {
  const id = instance.id;
  const run: MockRun = { state: 'checking', cancelled: false, updateOnly: false, answer: null };
  state.runs.set(id, run);
  event({ type: 'run_state', instance: id, state: 'checking', exit: null, update: null });
  for (let n = 0; n < 5; n++) {
    if (run.cancelled) return;
    event({ type: 'progress', instance: id, task: { key: 'server_pack_wait', params: { name: instance.name } }, done: 0, total: 0, bytes: 0 });
    await wait(400);
  }
  if (run.cancelled || state.runs.get(id) !== run) return;
  state.runs.delete(id);
  // No `MockPack` for a server instance, so the plain script runs it: queued, preparing, running.
  void scriptRun(state, instance);
}

/** Change an instance's mods folder. A pack's "Your additions" are jars in that folder, so they follow. */
function editMods(state: MockState, id: string, change: (list: LocalMod[]) => LocalMod[]): LocalMod[] {
  const list = change(state.mods.get(id) ?? []);
  state.mods.set(id, list);
  const pack = state.packs.get(id);
  if (pack) pack.contents.additions = change(pack.contents.additions);
  return list;
}

function planFor(state: MockState, request: BrowsePlanRequest): InstallPlanView {
  const project = findProject(request.project);
  if (!project) fail('pack', 'content_not_found', "That project wasn't found");
  const { detail } = project;
  if (detail.kind === 'modpack') fail('pack', 'content_incompatible', 'Modpacks become instances', { name: detail.title });
  const targetId = request.target.kind === 'instance' ? request.target.id : null;
  const instance = targetId ? state.listing.instances.find((i) => i.id === targetId) : undefined;
  if (targetId && !instance) fail('disk', 'instance_not_found', 'No such instance');
  if (instance) {
    const fit = fitFor(project.versions, detail.kind, instance);
    if (!fit.ok)
      fail('pack', fit.reason === 'needsLoader' ? 'content_needs_loader' : 'content_incompatible', 'Not for this instance', {
        name: detail.title,
      });
  }
  const have = instance ? (state.browse.installed.get(instance.id) ?? new Set<string>()) : new Set<string>();
  if (have.has(detail.projectId)) fail('pack', 'content_already_installed', 'Already there', { name: detail.title });
  const item = (p: MockProject, reason: PlanItemView['reason']): PlanItemView => ({
    projectId: p.detail.projectId,
    title: p.detail.title,
    kind: p.detail.kind,
    versionNumber: p.versions[0]!.number,
    fileName: `${p.detail.slug}.${p.detail.kind === 'mod' ? 'jar' : 'zip'}`,
    size: p.versions[0]!.size ?? 0,
    reason,
  });
  const items = [item(project, { kind: 'requested' })];
  const skipped: PlanSkip[] = [];
  const queue = [...project.deps.map((id) => ({ id, of: detail.title }))];
  if (detail.kind === 'shader') queue.push({ id: 'YL57xq9U', of: '' });
  const seen = new Set([detail.projectId]);
  while (queue.length > 0) {
    const next = queue.shift()!;
    if (seen.has(next.id)) continue;
    seen.add(next.id);
    const dep = findProject(next.id)!;
    if (have.has(next.id)) {
      skipped.push({ projectId: next.id, title: dep.detail.title, reason: 'installed' });
      continue;
    }
    items.push(item(dep, next.of === '' ? { kind: 'shader_mod' } : { kind: 'dependency', of: next.of }));
    queue.push(...dep.deps.map((id) => ({ id, of: dep.detail.title })));
  }
  const version = request.version ? project.versions.find((v) => v.id === request.version) : pickDefault(project.versions);
  const minecraft = instance?.game.minecraft ?? version?.gameVersions.at(-1) ?? '1.21.1';
  const target: InstallPlanView['target'] = instance
    ? { kind: 'instance', id: instance.id, name: instance.name, minecraft, loader: instance.game.loader?.kind ?? null }
    : { kind: 'new', name: detail.title, minecraft, loader: detail.kind === 'resource_pack' ? null : 'fabric' };
  const planId = `plan-${state.browse.nextPlan++}`;
  const view: InstallPlanView = { planId, target, items, skipped, totalSize: items.reduce((sum, i) => sum + i.size, 0) };
  state.browse.plans.set(planId, { view, kind: detail.kind });
  return view;
}

async function handle(state: MockState, cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const find = (id: unknown) => state.listing.instances.find((i) => i.id === id);
  switch (cmd) {
    case 'app_info':
      return state.appInfo;
    case 'settings_get':
      return state.settings;
    case 'settings_set':
      state.settings = args.settings as Settings;
      return state.settings;
    case 'update_status':
      return state.update;
    case 'update_check':
      if (state.update.state === 'off' || state.update.state === 'ready') return state.update;
      state.update = { state: 'up_to_date' };
      event({ type: 'update', state: state.update });
      return state.update;
    case 'update_restart':
      if (state.runs.size > 0) fail('game', 'launcher_update_busy', 'Close your games before restarting to update');
      return null;
    case 'instances_list':
      return state.listing;
    case 'instance_create': {
      const request = args.request as NewInstance;
      let id = slug(request.name);
      for (let n = 2; find(id); n++) id = `${slug(request.name)}-${n}`;
      const created: Instance = {
        id,
        name: request.name.trim(),
        source: { kind: 'none' },
        game: { minecraft: request.minecraft, loader: request.loader },
        updatePolicy: 'ask',
        optionalChoices: {},
        java: { path: null, memoryMb: state.settings.defaultMemoryMb },
        server: null,
      };
      state.listing.instances = [...state.listing.instances, created].sort((a, b) => a.name.localeCompare(b.name));
      return created;
    }
    case 'instance_update': {
      const updated = args.instance as Instance;
      state.listing.instances = state.listing.instances.map((i) => (i.id === updated.id ? updated : i));
      return updated;
    }
    case 'instance_rename': {
      const target = find(args.id) ?? fail('disk', 'instance_not_found', 'gone');
      const renamed: Instance = { ...target, name: String(args.name).trim() };
      state.listing.instances = state.listing.instances.map((i) => (i.id === renamed.id ? renamed : i));
      return renamed;
    }
    case 'instance_delete':
      state.listing.instances = state.listing.instances.filter((i) => i.id !== args.id);
      state.listing.damaged = state.listing.damaged.filter((d) => d.id !== args.id);
      return null;
    case 'catalog_minecraft':
      return mcVersionsFx;
    case 'catalog_loaders': {
      const kind = String(args.kind);
      if (kind === 'forge') {
        return [
          { version: '47.4.22', stable: true },
          { version: '47.4.20', stable: true },
        ];
      }
      if (kind === 'neoforge') return [{ version: '21.1.209', stable: true }];
      return loaderVersionsFx;
    }
    case 'accounts_list':
      return state.accounts;
    case 'account_add_offline': {
      if (!state.appInfo.offlineAllowed) fail('auth', 'offline_locked', 'Offline accounts are locked');
      const name = String(args.name);
      if (!/^\w{3,16}$/.test(name)) fail('auth', 'offline_name_invalid', 'bad name');
      const uuid = `mock-${name.toLowerCase()}`;
      state.accounts.accounts = [...state.accounts.accounts.filter((a) => a.uuid !== uuid), { provider: 'offline', username: name, uuid }];
      state.accounts.active = uuid;
      return state.accounts;
    }
    case 'account_set_active':
      state.accounts.active = String(args.uuid);
      return state.accounts;
    case 'account_remove': {
      state.accounts.accounts = state.accounts.accounts.filter((a) => a.uuid !== args.uuid);
      if (state.accounts.active === args.uuid) state.accounts.active = state.accounts.accounts[0]?.uuid ?? null;
      return state.accounts;
    }
    case 'play': {
      const target = find(args.id) ?? fail('disk', 'instance_not_found', 'gone');
      if (!state.accounts.active) fail('auth', 'no_account', 'Add an account before playing');
      if (state.runs.has(target.id)) fail('game', 'already_running', 'already running');
      if (state.servers?.scenario === 'pending' && target.id === MOCK_SERVER_ID) {
        void scriptServerWait(state, target);
        return null;
      }
      void scriptRun(state, target);
      return null;
    }
    case 'play_answer': {
      const run = state.runs.get(String(args.id));
      if (run?.state === 'asking' && run.answer) {
        run.answer(Boolean(args.update));
        run.answer = null;
      }
      return null;
    }
    case 'pack_update': {
      const target = find(args.id) ?? fail('disk', 'instance_not_found', 'gone');
      if (!state.packs.has(target.id)) fail('pack', 'no_pack', "This instance doesn't come from a pack");
      if (state.runs.has(target.id)) fail('game', 'already_running', 'already running');
      void scriptRun(state, target, true);
      return null;
    }
    case 'pack_status': {
      const pack = state.packs.get(String(args.id)) ?? fail('pack', 'no_pack', "This instance doesn't come from a pack");
      return { installedVersion: pack.installed, update: pack.installed ? pack.update : null, checkError: null } satisfies PackStatus;
    }
    case 'pack_contents':
      return (state.packs.get(String(args.id)) ?? fail('pack', 'no_pack', 'no pack')).contents;
    case 'pack_set_optional': {
      const id = String(args.id);
      const pack = state.packs.get(id) ?? fail('pack', 'no_pack', 'no pack');
      const target = find(id) ?? fail('disk', 'instance_not_found', 'gone');
      const mod = pack.contents.managed.find((m) => m.path === args.path && m.optional);
      if (!mod?.optional) fail('disk', 'mod_not_found', 'not optional', { file: String(args.path) });
      mod.optional.enabled = Boolean(args.enabled);
      const updated: Instance = { ...target, optionalChoices: { ...target.optionalChoices, [String(args.path)]: Boolean(args.enabled) } };
      state.listing.instances = state.listing.instances.map((i) => (i.id === id ? updated : i));
      return updated;
    }
    case 'manual_import': {
      const pack = state.packs.get(String(args.id));
      if (!pack || pack.manual.length === 0) return { found: 0, total: 0, remaining: [] } satisfies ManualProgress;
      pack.found = Math.min(pack.found + 1, pack.manual.length);
      return { found: pack.found, total: pack.manual.length, remaining: pack.manual.slice(pack.found) } satisfies ManualProgress;
    }
    case 'pack_art':
      return state.packs.get(String(args.id))?.art ?? ({ icon: null, banner: null } satisfies PackArtData);
    case 'pack_add': {
      const request = args.request as PackAddRequest;
      if (request.kind === 'link' && !/^(https:\/\/|modrinth:)/.test(request.url.trim())) {
        fail('pack', 'pack_link_invalid', "That doesn't look like a pack link");
      }
      // `modrinth:<project>@<version>` links name the catalog's project.
      const modrinth = request.kind === 'link' ? /^modrinth:([^@\s]+)/.exec(request.url.trim()) : null;
      const catalogName = modrinth ? findProject(modrinth[1]!)?.detail.title : undefined;
      const name =
        request.kind === 'file' ? (request.path.split(/[\\/]/).pop() ?? 'Pack').replace(/\.mrpack$/i, '') : (catalogName ?? 'Asalith');
      let id = slug(name);
      for (let n = 2; find(id); n++) id = `${slug(name)}-${n}`;
      const created: Instance = {
        id,
        name,
        source:
          request.kind === 'file'
            ? { kind: 'mrpack', name: `${name}.mrpack`, sha1: '0'.repeat(40) }
            : { kind: 'packwiz', url: request.url.trim() },
        game: { minecraft: '1.20.1', loader: { kind: 'forge', version: '47.4.22' } },
        updatePolicy: 'ask',
        optionalChoices: {},
        java: { path: null, memoryMb: state.settings.defaultMemoryMb },
        server: null,
      };
      state.listing.instances = [...state.listing.instances, created].sort((a, b) => a.name.localeCompare(b.name));
      state.packs.set(id, { installed: null, update: null, contents: { managed: [], additions: [] }, art: SKY_ART, manual: [], found: 0 });
      return created;
    }
    case 'browse_search': {
      const request = args.request as SearchRequest;
      if (request.query === 'offline') fail('network', 'host_unreachable', "Couldn't reach api.modrinth.com", { host: 'api.modrinth.com' });
      if (request.query === 'busy') fail('network', 'browse_rate_limited', 'Too many searches', { seconds: '3' });
      const scoped = request.instance ? find(request.instance) : undefined;
      const matches = CATALOG.filter(
        (p) =>
          p.detail.kind === request.kind &&
          p.detail.title.toLowerCase().includes(request.query.trim().toLowerCase()) &&
          request.categories.every((c) => p.detail.categories.includes(c)) &&
          (!scoped || p.versions.some((v) => fitsVersion(v, p.detail.kind, scoped))),
      );
      const hits: SearchHit[] = matches.slice(request.offset, request.offset + 20).map((p) => ({
        projectId: p.detail.projectId,
        slug: p.detail.slug,
        title: p.detail.title,
        author: p.detail.author ?? '',
        summary: p.detail.summary,
        iconUrl: p.detail.iconUrl,
        downloads: p.detail.downloads,
        follows: p.detail.follows,
        categories: p.detail.categories,
        kind: p.detail.kind,
      }));
      return { hits, total: matches.length, offset: request.offset } satisfies SearchPage;
    }
    case 'browse_categories':
      return args.kind === 'mod' ? ['library', 'optimization', 'technology'] : ['adventure'];
    case 'browse_project': {
      const project = findProject(args.id as string);
      if (!project) fail('pack', 'content_not_found', "That project wasn't found");
      return project.detail;
    }
    case 'browse_versions':
      return findProject(args.project as string)?.versions ?? [];
    case 'browse_image':
      return MOCK_ICON;
    case 'browse_installed':
      return [...(state.browse.installed.get(args.id as string) ?? [])].sort();
    case 'browse_plan':
      return planFor(state, args.request as BrowsePlanRequest);
    case 'browse_install': {
      const stored = state.browse.plans.get(args.planId as string);
      if (!stored) fail('pack', 'content_plan_expired', 'That took too long');
      state.browse.plans.delete(args.planId as string);
      const { view } = stored;
      let instance: Instance;
      if (view.target.kind === 'instance') {
        instance = find(view.target.id) ?? fail('disk', 'instance_not_found', 'gone');
      } else {
        instance = (await handle(state, 'instance_create', {
          request: {
            name: view.target.name,
            minecraft: view.target.minecraft,
            loader: view.target.loader ? { kind: view.target.loader, version: '0.16.14' } : null,
          },
        })) as Instance;
      }
      const have = state.browse.installed.get(instance.id) ?? new Set<string>();
      view.items.forEach((item) => have.add(item.projectId));
      for (const item of view.items) {
        if (item.kind !== 'resource_pack' && item.kind !== 'shader') continue;
        const key = contentKey(instance.id, item.kind);
        const files = state.content.get(key) ?? [];
        if (!files.some((f) => f.fileName === item.fileName)) {
          state.content.set(key, [...files, { fileName: item.fileName, size: item.size, folder: false, managed: false }]);
        }
      }
      state.browse.installed.set(instance.id, have);
      const task = { key: 'install_content', params: { name: view.items[0]!.title } };
      event({ type: 'progress', instance: instance.id, task, done: view.items.length, total: view.items.length, bytes: view.totalSize });
      return { instance, added: view.items.map((i) => i.fileName) } satisfies InstallResult;
    }
    case 'open_link':
      state.browse.opened.push(args.url as string);
      return null;
    case 'deep_link_take': {
      const link = state.pendingLink;
      state.pendingLink = null;
      const invite = link ? inviteCanonical(link) : null;
      const taken: DeepLink | null = invite ? { kind: 'join', invite } : link ? { kind: 'pack', url: link } : null;
      return taken;
    }
    case 'stop': {
      const id = String(args.id);
      const run = state.runs.get(id);
      if (!run) return null;
      if (['checking', 'asking', 'queued', 'preparing'].includes(run.state)) {
        run.cancelled = true;
        run.answer?.(false);
        state.runs.delete(id);
        event({ type: 'run_state', instance: id, state: 'idle', exit: exitOf({ cancelled: true }), update: null });
      } else if (run.state === 'syncing') {
        // Like the real sync: it stops at its next safe point, then the run ends cancelled.
        run.cancelled = true;
        setState(state, id, 'stopping');
        setTimeout(() => {
          if (state.runs.get(id) !== run) return;
          state.runs.delete(id);
          event({ type: 'run_state', instance: id, state: 'idle', exit: exitOf({ cancelled: true, updateOnly: run.updateOnly }), update: null });
        }, 300);
      } else if (run.state === 'running') {
        setState(state, id, 'stopping');
        setTimeout(() => {
          if (state.runs.get(id) !== run) return;
          state.runs.delete(id);
          event({ type: 'run_state', instance: id, state: 'idle', exit: exitOf({ killed: true }), update: null });
        }, 400);
      }
      return null;
    }
    case 'run_states':
      return [...state.runs].map(([instance, run]) => ({
        instance,
        state: run.state,
        update: run.state === 'asking' ? (state.packs.get(instance)?.update ?? null) : null,
      }));
    case 'log_tail':
      return [];
    case 'mods_list':
      return state.mods.get(String(args.id)) ?? [];
    case 'mod_set_enabled': {
      const file = String(args.file);
      const base = file.replace(/\.disabled$/, '');
      return editMods(state, String(args.id), (list) =>
        list.map((m) => (m.fileName === file ? { ...m, fileName: args.enabled ? base : `${base}.disabled`, enabled: Boolean(args.enabled) } : m)),
      );
    }
    case 'mods_add': {
      const added = (args.paths as string[]).map((p) => ({ fileName: p.split(/[\\/]/).pop() ?? p, enabled: true, size: 1_200_000 }));
      return editMods(state, String(args.id), (list) => [...list, ...added].sort((a, b) => a.fileName.localeCompare(b.fileName)));
    }
    case 'mod_remove':
      return editMods(state, String(args.id), (list) => list.filter((m) => m.fileName !== args.file));
    case 'content_list':
      return clone(state.content.get(contentKey(String(args.id), args.kind as ContentKind)) ?? []);
    case 'content_remove': {
      const key = contentKey(String(args.id), args.kind as ContentKind);
      const files = state.content.get(key) ?? [];
      const file = files.find((f) => f.fileName === args.file);
      if (file?.managed) fail('disk', 'mod_managed', 'That file comes with the pack', { file: file.fileName });
      if (file?.folder) fail('disk', 'mod_name_invalid', "That isn't a pack file name");
      const left = files.filter((f) => f.fileName !== args.file);
      state.content.set(key, left);
      return clone(left);
    }
    case 'diagnostics_reveal':
    case 'open_folder':
    case 'quit':
      return null;
    case 'java_test':
      return { major: 17 };
    case 'plugin:dialog|open': {
      // Mirrors the real dialog: an array when `multiple` is set, otherwise one path.
      const options = args.options as { multiple?: boolean; filters?: { extensions?: string[] }[] } | undefined;
      if (options?.filters?.[0]?.extensions?.includes('mrpack')) return 'C:/Users/player/Downloads/Sky Islands 1.0.4.mrpack';
      const path = 'C:/Users/player/Downloads/lithium-fabric-0.11.2.jar';
      return options?.multiple ? [path] : path;
    }
    case 'diagnostics_export':
      return (args as { path: string }).path;
    case 'plugin:dialog|save':
      return state.options.savePath === undefined ? 'C:/Users/player/Downloads/perland-diagnostics.zip' : state.options.savePath;
    case 'plugin:path|resolve_directory':
      return 'C:/Users/player/Downloads';
    case 'plugin:path|join':
      return ((args.paths as string[]) ?? []).join('/');
    case 'plugin:window|is_maximized':
      return false;
    default: {
      const served = handleServer(state.servers, state.listing, cmd, args);
      if (served !== NOT_MINE) return served;
      if (cmd.startsWith('plugin:')) return null;
      return fail('game', 'internal', `mock has no command ${cmd}`);
    }
  }
}

/** Every backend created so far, so tests can stop their scripted runs at teardown. */
const backends = new Set<MockState>();

/** Stop every scripted run. A run left going outlives its test and emits after the IPC mocks are gone. */
export function stopMockRuns() {
  for (const state of backends) {
    for (const run of state.runs.values()) {
      run.cancelled = true;
      run.answer?.(false);
    }
    state.runs.clear();
  }
  backends.clear();
}

export function createMockBackend(options: MockOptions = {}) {
  const state = seed(options);
  backends.add(state);
  return {
    state,
    // Real IPC hands back fresh objects; a shared reference would make TanStack Query see "no change".
    handle: async (cmd: string, args?: unknown) => clone(await handle(state, cmd, (args ?? {}) as Record<string, unknown>)),
  };
}

export function installMock(options: MockOptions = {}) {
  const backend = createMockBackend(options);
  mockWindows('main');
  mockIPC((cmd, args) => backend.handle(cmd, args), { shouldMockEvents: true });
  return backend;
}
