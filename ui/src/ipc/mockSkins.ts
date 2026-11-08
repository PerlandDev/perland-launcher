// Mock skin wardrobes for `pnpm --filter ui dev:mock` and the tests: in memory, one per account (`owner`), with a scripted
// current skin and upload. Like the core, the active account adopts loose skins when it lists its wardrobe.
import skinEntriesFx from './contract/skin_entries.json';
import skinVersionsFx from './contract/skin_versions.json';
import { encodeTexture } from '../features/skins/codec';
import { blankSkin, starterSkin } from '../features/skins/starter';
import type { AccountsFile, CurrentSkin, LauncherError, SkinEntry, SkinModel, SkinVersion, SkinVersionReason } from './schema';

export const NOT_SKINS = Symbol('not a skin command');

/** `?skins=`: `seeded` (the default: two skins in Steve's wardrobe, and a Microsoft account wears its own skin), `empty`,
 *  `rejected` (every upload is refused), `loose` (only an untouched Perlander saved with no account signed in, from before
 *  per-account wardrobes) or `default` (a Microsoft account wears Minecraft's default skin). */
export type SkinsScenario = 'seeded' | 'empty' | 'rejected' | 'loose' | 'default';

export interface SkinsState {
  entries: SkinEntry[];
  textures: Map<string, string>;
  scenario: SkinsScenario;
  /** What `skin_current` answers for a Microsoft account. */
  current: CurrentSkin | null;
  uploads: { id: string; uuid: string; model: SkinModel }[];
  /** Each skin's kept versions, oldest first, with their textures. */
  versions: Map<string, KeptVersion[]>;
  seq: number;
}

interface KeptVersion {
  id: string;
  taken: number;
  reason: SkinVersionReason;
  worn: boolean;
  texture: string;
}

/** A save this long after the skin's last change starts a new editing session (as in Rust). */
const IDLE_SECS = 5 * 60;
const VERSIONS_CAP = 30;

const PNG_URL = 'data:image/png;base64,';

function fail(category: LauncherError['category'], code: string, message: string, params: Record<string, string> = {}): never {
  throw { category, code, message, fix: null, detail: null, params } satisfies LauncherError;
}

/** A skin drawn by the starter code (dev:mock has a canvas), else a placeholder data URL (tests). */
function drawn(kind: 'starter' | 'blank', model: SkinModel): string {
  try {
    return encodeTexture((kind === 'starter' ? starterSkin(model) : blankSkin(model)).data);
  } catch {
    return `${PNG_URL}AAAA`;
  }
}

/** The skin the mock Microsoft account really wears: the Perlander in a purple shirt, so it can't be mistaken for him. */
function accountSkin(): string {
  try {
    const px = starterSkin('slim').data;
    for (let i = 0; i < px.length; i += 4) {
      const [r, g, b] = [px[i]!, px[i + 1]!, px[i + 2]!];
      if (g > r && g > b) [px[i + 1], px[i + 2]] = [b, g];
    }
    return encodeTexture(px);
  } catch {
    return `${PNG_URL}REAL`;
  }
}

/** The Perlander with its shirt in another colour (dev:mock has a canvas), else a placeholder named after `seed`. */
function variant(seed: number, model: SkinModel): string {
  try {
    const px = starterSkin(model);
    const shirts: [number, number, number][] = [[78, 140, 214], [214, 92, 74], [232, 190, 60], [150, 96, 200]];
    const [r, g, b] = shirts[seed % shirts.length]!;
    const d = px.data;
    for (let i = 0; i < d.length; i += 4) {
      // The lime shirt: green well above red and blue.
      if (d[i + 3]! > 0 && d[i + 1]! > d[i]! + 30 && d[i + 1]! > d[i + 2]! + 30) {
        const k = d[i + 1]! / 171;
        d[i] = Math.min(255, r * k);
        d[i + 1] = Math.min(255, g * k);
        d[i + 2] = Math.min(255, b * k);
      }
    }
    return encodeTexture(d);
  } catch {
    return `${PNG_URL}VERSION${seed}`;
  }
}

export function seedSkins(scenario: SkinsScenario = 'seeded'): SkinsState {
  const entries: SkinEntry[] =
    scenario === 'empty' || scenario === 'default'
      ? []
      : scenario === 'loose'
        ? [{ id: 'skin-00000000aaaa', name: 'The Perlander', model: 'classic', updated: 1_762_300_000 }]
        : (skinEntriesFx as SkinEntry[]).map((e) => ({ ...e }));
  const textures = new Map(entries.map((e, i) => [e.id, drawn(i === 0 ? 'starter' : 'blank', e.model)]));
  const current = scenario === 'default' ? null : { texture: accountSkin(), model: 'slim' as const };
  const versions = new Map<string, KeptVersion[]>();
  const first = entries[0];
  if (first) {
    // The fixture's versions, newest first; the newest is the skin as it is now.
    // Times count back from now (20 minutes, a day, three days, four days), so the list reads like a real one.
    const ago = [20 * 60, 26 * 3600, 3 * 86400 + 5000, 4 * 86400 + 9000];
    const now = Math.floor(Date.now() / 1000);
    const kept = (skinVersionsFx as SkinVersion[]).map(({ current, ...v }, i) => ({
      ...v,
      taken: now - (ago[i] ?? 0),
      texture: current ? textures.get(first.id)! : variant(i, first.model),
    }));
    versions.set(first.id, kept.reverse());
  }
  return { entries, textures, scenario, current, uploads: [], versions, seq: 0 };
}

function keep(state: SkinsState, entry: SkinEntry, texture: string, reason: SkinVersionReason, taken = entry.updated) {
  const list = state.versions.get(entry.id) ?? [];
  const newest = list[list.length - 1];
  if (newest && newest.texture === texture) {
    if (reason === 'worn') newest.worn = true;
    return;
  }
  const last = newest ? Number(newest.id) : 0;
  list.push({ id: String(Math.max(Date.now(), last + 1)), taken, reason, worn: reason === 'worn', texture });
  const unworn = list.filter((v) => !v.worn);
  const drop = new Set(unworn.slice(0, Math.max(0, unworn.length - VERSIONS_CAP)).map((v) => v.id));
  state.versions.set(entry.id, list.filter((v) => !drop.has(v.id)));
}

function findVersion(state: SkinsState, entry: SkinEntry, version: unknown): KeptVersion {
  const found = state.versions.get(entry.id)?.find((v) => v.id === version);
  if (!found) fail('disk', 'skin_version_not_found', "That version of the skin isn't kept anymore.", { id: entry.id, version: String(version) });
  return found;
}

function find(state: SkinsState, id: unknown): SkinEntry {
  const entry = state.entries.find((e) => e.id === id);
  if (!entry) fail('disk', 'skin_not_found', "That skin isn't in your wardrobe anymore.", { id: String(id) });
  return entry;
}

function checkTexture(texture: unknown): string {
  if (typeof texture !== 'string' || !texture.startsWith(PNG_URL)) {
    fail('integrity', 'skin_invalid', "That file isn't a Minecraft skin.");
  }
  return texture;
}

function checkName(name: unknown): string {
  const trimmed = String(name ?? '').trim();
  if (trimmed.length < 1 || trimmed.length > 64) fail('disk', 'name_invalid', 'Names must be 1–64 characters.');
  return trimmed;
}

/** `owner`'s wardrobe (the loose skins for null), in the player's order: the order of `entries`, as in Rust. */
function wardrobe(state: SkinsState, owner: string | null) {
  return state.entries.filter((e) => (e.owner ?? null) === owner).map((e) => ({ ...e }));
}

function ownerArg(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function wornBy(state: SkinsState, id: string, uuid: string) {
  for (const e of state.entries) {
    if (e.id === id) e.wornBy = uuid;
    else if (e.wornBy === uuid) delete e.wornBy;
  }
}

/** A new skin goes on top of its wardrobe; a copy (`after` its source) right below the source. */
function add(state: SkinsState, name: string, model: SkinModel, texture: string, owner: string | null, seeded = false, after?: SkinEntry): SkinEntry {
  state.seq += 1;
  const entry: SkinEntry = {
    id: `skin-${state.seq.toString(16).padStart(12, '0')}`,
    name,
    model,
    updated: Math.floor(Date.now() / 1000) + state.seq,
    ...(owner ? { owner } : {}),
    ...(seeded ? { seeded } : {}),
  };
  state.entries.splice(after ? state.entries.indexOf(after) + 1 : 0, 0, entry);
  state.textures.set(entry.id, texture);
  return entry;
}

function touch(entry: SkinEntry) {
  delete entry.seeded;
  entry.updated = Math.max(Math.floor(Date.now() / 1000), entry.updated + 1);
}

export function handleSkins(state: SkinsState, accounts: AccountsFile, cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case 'skins_list': {
      const owner = ownerArg(args.owner);
      if (owner && owner === accounts.active) {
        // An untouched starter is dropped, not adopted: one marked seeded, or one never worn that is the starter exactly.
        const starter = typeof args.starter === 'string' ? args.starter : null;
        const untouched = (e: SkinEntry) => e.seeded || (!e.wornBy && starter !== null && state.textures.get(e.id) === starter);
        for (const e of state.entries) if (e.owner === undefined && untouched(e)) state.versions.delete(e.id);
        state.entries = state.entries.filter((e) => e.owner !== undefined || !untouched(e));
        for (const e of state.entries) e.owner ??= owner;
      }
      return wardrobe(state, owner);
    }
    case 'skin_texture':
      find(state, args.id);
      return state.textures.get(String(args.id));
    case 'skin_create':
      return add(state, checkName(args.name), args.model as SkinModel, checkTexture(args.texture), ownerArg(args.owner), args.seeded === true);
    case 'skin_import': {
      const path = String(args.path);
      if (!path.toLowerCase().endsWith('.png')) fail('integrity', 'skin_invalid', "That file isn't a Minecraft skin.");
      const stem = path.split(/[\\/]/).pop()!.replace(/\.png$/i, '') || 'Skin';
      return add(state, stem, 'classic', drawn('blank', 'classic'), ownerArg(args.owner));
    }
    case 'skin_save': {
      const entry = find(state, args.id);
      const texture = checkTexture(args.texture);
      const fresh = (state.versions.get(entry.id) ?? []).length === 0;
      if (fresh || Date.now() / 1000 - entry.updated >= IDLE_SECS) keep(state, entry, state.textures.get(entry.id)!, 'edited');
      state.textures.set(entry.id, texture);
      touch(entry);
      return { ...entry };
    }
    case 'skin_update': {
      const entry = find(state, args.id);
      if (args.name != null) entry.name = checkName(args.name);
      if (args.model != null) entry.model = args.model as SkinModel;
      touch(entry);
      return { ...entry };
    }
    case 'skin_duplicate': {
      const entry = find(state, args.id);
      return add(state, checkName(args.name), entry.model, state.textures.get(entry.id)!, entry.owner ?? null, false, entry);
    }
    case 'skin_reorder': {
      // As in Rust: the wardrobe's skins fill their own places in the list, those named first, the rest after them.
      const owner = ownerArg(args.owner);
      const ids = Array.isArray(args.ids) ? args.ids.map(String) : [];
      const mine = (e: SkinEntry) => (e.owner ?? null) === owner;
      const places = state.entries.flatMap((e, i) => (mine(e) ? [i] : []));
      const rest = places.map((i) => state.entries[i]!);
      const order: SkinEntry[] = [];
      for (const id of ids) {
        const at = rest.findIndex((e) => e.id === id);
        if (at >= 0) order.push(...rest.splice(at, 1));
      }
      order.push(...rest);
      places.forEach((i, k) => (state.entries[i] = order[k]!));
      return wardrobe(state, owner);
    }
    case 'skin_delete':
      find(state, args.id);
      state.entries = state.entries.filter((e) => e.id !== args.id);
      state.textures.delete(String(args.id));
      state.versions.delete(String(args.id));
      return null;
    case 'skin_versions': {
      const entry = find(state, args.id);
      // Only the newest version with the texture now is current, as in Rust.
      let now = state.textures.get(entry.id);
      return [...(state.versions.get(entry.id) ?? [])].reverse().map(({ texture, ...v }) => {
        const current = texture === now;
        if (current) now = undefined;
        return { ...v, current } satisfies SkinVersion;
      });
    }
    case 'skin_version_texture':
      return findVersion(state, find(state, args.id), args.version).texture;
    case 'skin_restore': {
      const entry = find(state, args.id);
      const version = findVersion(state, entry, args.version);
      keep(state, entry, state.textures.get(entry.id)!, 'edited');
      state.textures.set(entry.id, version.texture);
      touch(entry);
      keep(state, entry, version.texture, 'restored');
      return { ...entry };
    }
    case 'skin_version_copy': {
      const entry = find(state, args.id);
      const version = findVersion(state, entry, args.version);
      // The copy stays in the source's wardrobe and is never seeded, as in Rust.
      return add(state, checkName(args.name), entry.model, version.texture, entry.owner ?? null);
    }
    case 'skin_checkpoint': {
      const entry = find(state, args.id);
      keep(state, entry, state.textures.get(entry.id)!, 'edited');
      return null;
    }
    case 'skin_export': {
      const entry = find(state, args.id);
      keep(state, entry, state.textures.get(entry.id)!, 'exported');
      return String(args.path).toLowerCase().endsWith('.png') ? args.path : `${String(args.path)}.png`;
    }
    case 'skin_current': {
      const account = accounts.accounts.find((a) => a.uuid === args.uuid);
      if (!account) fail('auth', 'no_account', "That account isn't in the launcher anymore.");
      return account.provider === 'microsoft' ? state.current : null;
    }
    case 'skin_sync_current': {
      const uuid = String(args.uuid);
      const account = accounts.accounts.find((a) => a.uuid === uuid);
      if (!account) fail('auth', 'no_account', "That account isn't in the launcher anymore.");
      const current = state.current;
      if (account.provider !== 'microsoft' || !current) return null;
      const found =
        state.entries.find((e) => e.owner === uuid && e.model === current.model && state.textures.get(e.id) === current.texture) ??
        add(state, checkName(args.name), current.model, current.texture, uuid);
      wornBy(state, found.id, uuid);
      return { ...found };
    }
    case 'skin_wear': {
      const entry = find(state, args.id);
      const uuid = String(args.uuid);
      const account = accounts.accounts.find((a) => a.uuid === uuid);
      if (!account) fail('auth', 'no_account', "That account isn't in the launcher anymore.");
      if (account.provider !== 'microsoft') {
        fail('auth', 'skin_offline_account', "Offline accounts can't change their skin in Minecraft.");
      }
      if (state.scenario === 'rejected') fail('auth', 'skin_rejected', "Minecraft didn't accept this skin.");
      state.uploads.push({ id: entry.id, uuid, model: entry.model });
      wornBy(state, entry.id, uuid);
      state.current = { texture: state.textures.get(entry.id)!, model: entry.model };
      keep(state, entry, state.textures.get(entry.id)!, 'worn');
      return wardrobe(state, uuid);
    }
    default:
      return NOT_SKINS;
  }
}
