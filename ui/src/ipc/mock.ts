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
import mcVersionsFx from './contract/mc_versions.json';
import settingsFx from './contract/settings.json';
import { APP_EVENT } from './events';
import type {
  AccountsFile,
  AppEvent,
  AppInfo,
  ExitInfo,
  Instance,
  InstanceListing,
  LauncherError,
  LocalMod,
  NewInstance,
  RunStateKind,
  Settings,
} from './schema';

export interface MockOptions {
  firstRun?: boolean;
  crash?: boolean;
  release?: boolean;
}

interface MockRun {
  state: RunStateKind;
  cancelled: boolean;
}

const clone = <T>(value: T): T => structuredClone(value);
const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function fail(category: LauncherError['category'], code: string, message: string, params = {}): never {
  throw { category, code, message, fix: null, detail: null, params } satisfies LauncherError;
}

function slug(name: string) {
  const s = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  return s.slice(0, 48) || 'instance';
}

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
    build: options.release ? 'release' : 'dev',
    offlineAllowed: !options.release,
  };
  const mods = new Map<string, LocalMod[]>([['asalith-fields', clone(localModsFx) as LocalMod[]]]);
  return { listing, accounts, settings, appInfo, mods, runs: new Map<string, MockRun>(), options };
}

export type MockState = ReturnType<typeof seed>;

function event(e: AppEvent) {
  void emit(APP_EVENT, e);
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

async function scriptPlay(state: MockState, instance: Instance) {
  const id = instance.id;
  const run: MockRun = { state: 'queued', cancelled: false };
  state.runs.set(id, run);
  event({ type: 'run_state', instance: id, state: 'queued', exit: null, update: null });
  await wait(300);
  if (run.cancelled) return;
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
    await wait(60);
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
        exit: exitOf({ code: 1, crash: 'out_of_memory' }),
        update: null,
      });
      return;
    }
    await wait(400);
  }
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
    case 'catalog_loaders':
      return loaderVersionsFx;
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
      void scriptPlay(state, target);
      return null;
    }
    case 'stop': {
      const id = String(args.id);
      const run = state.runs.get(id);
      if (!run) return null;
      if (run.state === 'queued' || run.state === 'preparing') {
        run.cancelled = true;
        state.runs.delete(id);
        event({ type: 'run_state', instance: id, state: 'idle', exit: exitOf({ cancelled: true }), update: null });
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
      return [...state.runs].map(([instance, run]) => ({ instance, state: run.state, update: null }));
    case 'log_tail':
      return [];
    case 'mods_list':
      return state.mods.get(String(args.id)) ?? [];
    case 'mod_set_enabled': {
      const id = String(args.id);
      const file = String(args.file);
      const base = file.replace(/\.disabled$/, '');
      const list = (state.mods.get(id) ?? []).map((m) =>
        m.fileName === file ? { ...m, fileName: args.enabled ? base : `${base}.disabled`, enabled: Boolean(args.enabled) } : m,
      );
      state.mods.set(id, list);
      return list;
    }
    case 'mods_add': {
      const id = String(args.id);
      const added = (args.paths as string[]).map((p) => ({ fileName: p.split(/[\\/]/).pop() ?? p, enabled: true, size: 1_200_000 }));
      const list = [...(state.mods.get(id) ?? []), ...added].sort((a, b) => a.fileName.localeCompare(b.fileName));
      state.mods.set(id, list);
      return list;
    }
    case 'mod_remove': {
      const id = String(args.id);
      const list = (state.mods.get(id) ?? []).filter((m) => m.fileName !== args.file);
      state.mods.set(id, list);
      return list;
    }
    case 'open_folder':
    case 'quit':
      return null;
    case 'java_test':
      return { major: 17 };
    case 'plugin:dialog|open': {
      // Mirrors the real dialog: an array when `multiple` is set, otherwise one path.
      const path = 'C:/Users/player/Downloads/lithium-fabric-0.11.2.jar';
      return (args.options as { multiple?: boolean } | undefined)?.multiple ? [path] : path;
    }
    case 'plugin:window|is_maximized':
      return false;
    default:
      if (cmd.startsWith('plugin:')) return null;
      return fail('game', 'internal', `mock has no command ${cmd}`);
  }
}

/** Every backend created so far, so tests can stop their scripted runs at teardown. */
const backends = new Set<MockState>();

/** Stop every scripted run. A run left going outlives its test and emits after the IPC mocks are gone. */
export function stopMockRuns() {
  for (const state of backends) {
    for (const run of state.runs.values()) run.cancelled = true;
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
