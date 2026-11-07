// Mock skin wardrobe for `pnpm --filter ui dev:mock` and the tests: in memory, with a scripted current skin and upload.
import skinEntriesFx from './contract/skin_entries.json';
import { encodeTexture } from '../features/skins/codec';
import { blankSkin, starterSkin } from '../features/skins/starter';
import type { AccountsFile, CurrentSkin, LauncherError, SkinEntry, SkinModel } from './schema';

export const NOT_SKINS = Symbol('not a skin command');

/** `?skins=`: `seeded` (the default: two skins), `empty`, or `rejected` (every upload is refused). */
export type SkinsScenario = 'seeded' | 'empty' | 'rejected';

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

export function seedSkins(scenario: SkinsScenario = 'seeded'): SkinsState {
  const entries = scenario === 'empty' ? [] : (skinEntriesFx as SkinEntry[]).map((e) => ({ ...e }));
  const textures = new Map(entries.map((e, i) => [e.id, drawn(i === 0 ? 'starter' : 'blank', e.model)]));
  return { entries, textures, scenario, current: null, uploads: [], seq: 0 };
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

function sorted(state: SkinsState) {
  return [...state.entries].sort((a, b) => b.updated - a.updated);
}

function add(state: SkinsState, name: string, model: SkinModel, texture: string): SkinEntry {
  state.seq += 1;
  const entry: SkinEntry = {
    id: `skin-${state.seq.toString(16).padStart(12, '0')}`,
    name,
    model,
    updated: Math.floor(Date.now() / 1000) + state.seq,
  };
  state.entries.push(entry);
  state.textures.set(entry.id, texture);
  return entry;
}

function touch(entry: SkinEntry) {
  entry.updated = Math.max(Math.floor(Date.now() / 1000), entry.updated + 1);
}

export function handleSkins(state: SkinsState, accounts: AccountsFile, cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case 'skins_list':
      return sorted(state);
    case 'skin_texture':
      find(state, args.id);
      return state.textures.get(String(args.id));
    case 'skin_create':
      return add(state, checkName(args.name), args.model as SkinModel, checkTexture(args.texture));
    case 'skin_import': {
      const path = String(args.path);
      if (!path.toLowerCase().endsWith('.png')) fail('integrity', 'skin_invalid', "That file isn't a Minecraft skin.");
      const stem = path.split(/[\\/]/).pop()!.replace(/\.png$/i, '') || 'Skin';
      return add(state, stem, 'classic', drawn('blank', 'classic'));
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
      return add(state, checkName(args.name), entry.model, state.textures.get(entry.id)!);
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
      for (const e of state.entries) {
        if (e.id === entry.id) e.wornBy = uuid;
        else if (e.wornBy === uuid) delete e.wornBy;
      }
      state.current = { texture: state.textures.get(entry.id)!, model: entry.model };
      return sorted(state);
    }
    default:
      return NOT_SKINS;
  }
}
