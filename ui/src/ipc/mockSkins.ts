// Mock skin wardrobes for `pnpm --filter ui dev:mock` and the tests: in memory, one per account (`owner`), with a scripted
// current skin and upload. Like the core, the active account adopts loose skins when it lists its wardrobe.
import skinEntriesFx from './contract/skin_entries.json';
import { encodeTexture } from '../features/skins/codec';
import { blankSkin, starterSkin } from '../features/skins/starter';
import type { AccountsFile, CurrentSkin, LauncherError, SkinEntry, SkinModel } from './schema';

export const NOT_SKINS = Symbol('not a skin command');

/** `?skins=`: `seeded` (the default: two skins in Steve's wardrobe, and a Microsoft account wears its own skin), `empty`,
 *  `rejected` (every upload is refused), `loose` (only a Perlander made with no account signed in, as before per-account
 *  wardrobes) or `default` (a Microsoft account wears Minecraft's default skin). */
export type SkinsScenario = 'seeded' | 'empty' | 'rejected' | 'loose' | 'default';

export interface SkinsState {
  entries: SkinEntry[];
  textures: Map<string, string>;
  scenario: SkinsScenario;
  /** What `skin_current` answers for a Microsoft account. */
  current: CurrentSkin | null;
  uploads: { id: string; uuid: string; model: SkinModel }[];
  seq: number;
}

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

export function seedSkins(scenario: SkinsScenario = 'seeded'): SkinsState {
  const entries: SkinEntry[] =
    scenario === 'empty' || scenario === 'default'
      ? []
      : scenario === 'loose'
        ? [{ id: 'skin-00000000aaaa', name: 'The Perlander', model: 'classic', updated: 1_762_300_000, seeded: true }]
        : (skinEntriesFx as SkinEntry[]).map((e) => ({ ...e }));
  const textures = new Map(entries.map((e, i) => [e.id, drawn(i === 0 ? 'starter' : 'blank', e.model)]));
  const current = scenario === 'default' ? null : { texture: accountSkin(), model: 'slim' as const };
  return { entries, textures, scenario, current, uploads: [], seq: 0 };
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

/** `owner`'s wardrobe (the loose skins for null): the skin it wears first, then the newest. */
function wardrobe(state: SkinsState, owner: string | null) {
  const wears = (e: SkinEntry) => owner !== null && e.wornBy === owner;
  return state.entries
    .filter((e) => (e.owner ?? null) === owner)
    .sort((a, b) => Number(wears(b)) - Number(wears(a)) || b.updated - a.updated)
    .map((e) => ({ ...e }));
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

function add(state: SkinsState, name: string, model: SkinModel, texture: string, owner: string | null, seeded = false): SkinEntry {
  state.seq += 1;
  const entry: SkinEntry = {
    id: `skin-${state.seq.toString(16).padStart(12, '0')}`,
    name,
    model,
    updated: Math.floor(Date.now() / 1000) + state.seq,
    ...(owner ? { owner } : {}),
    ...(seeded ? { seeded } : {}),
  };
  state.entries.push(entry);
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
        // An untouched seeded starter is dropped, not adopted.
        state.entries = state.entries.filter((e) => e.owner !== undefined || !e.seeded);
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
      state.textures.set(entry.id, checkTexture(args.texture));
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
      return add(state, checkName(args.name), entry.model, state.textures.get(entry.id)!, entry.owner ?? null);
    }
    case 'skin_delete':
      find(state, args.id);
      state.entries = state.entries.filter((e) => e.id !== args.id);
      state.textures.delete(String(args.id));
      return null;
    case 'skin_export':
      find(state, args.id);
      return String(args.path).toLowerCase().endsWith('.png') ? args.path : `${String(args.path)}.png`;
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
      return wardrobe(state, uuid);
    }
    default:
      return NOT_SKINS;
  }
}
