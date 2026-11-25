import type { DroppedFile, DroppedKind, DropResult, DropSkipReason, InstanceListing } from '../../ipc/schema';
import type { ModsTab, Route } from '../../state/route';
import { pickId } from '../home/pick';

/** The kinds an instance takes, in the order a sentence lists them. */
export const ADDED_KINDS = ['mod', 'resource_pack', 'shader_pack'] as const;
export type AddedKind = (typeof ADDED_KINDS)[number];
export type KindCounts = Record<AddedKind, number>;

export interface DropPlan {
  /** A `.mrpack` dropped on its own: it starts an import instead of going into an instance. */
  modpack: DroppedFile | null;
  /** What the target will take. */
  adds: KindCounts;
  /** Mods and shader packs a vanilla target can't run. */
  needsLoader: number;
  /** What no instance takes: other files, zips that aren't packs, a modpack among other files. */
  other: number;
}

const isAdded = (kind: DroppedKind): kind is AddedKind => (ADDED_KINDS as readonly string[]).includes(kind);

/** What a drop will do. `modded` is the target's loader: null while the target is still to be chosen. */
export function planDrop(files: DroppedFile[], modded: boolean | null): DropPlan {
  const modpack = files.length === 1 && files[0]?.kind === 'modpack' ? files[0] : null;
  const plan: DropPlan = { modpack, adds: { mod: 0, resource_pack: 0, shader_pack: 0 }, needsLoader: 0, other: 0 };
  if (modpack) return plan;
  for (const file of files) {
    if (!isAdded(file.kind)) plan.other += 1;
    else if (modded === false && file.kind !== 'resource_pack') plan.needsLoader += 1;
    else plan.adds[file.kind] += 1;
  }
  return plan;
}

export const addsAnything = (counts: KindCounts) => ADDED_KINDS.some((kind) => counts[kind] > 0);

const UP_FRONT: Partial<Record<DroppedKind, DropSkipReason>> = { other: 'not_content', unknown_zip: 'not_a_pack', modpack: 'modpack' };

/** The files no instance would take, as the core would name them: for a drop with nothing to add and no target yet. */
export function skippedUpFront(files: DroppedFile[]): DropResult['skipped'] {
  return files.flatMap((file) => {
    const reason = UP_FRONT[file.kind];
    return reason ? [{ name: file.name, kind: file.kind, reason }] : [];
  });
}

/** Counts per kind of what was added. */
export function countAdded(added: DropResult['added']): KindCounts {
  const counts: KindCounts = { mod: 0, resource_pack: 0, shader_pack: 0 };
  for (const file of added) if (isAdded(file.kind)) counts[file.kind] += 1;
  return counts;
}

type Translate = (key: string, options?: { count?: number }) => string;

/** "2 mods and 1 shader pack": the kinds with a count, joined the way the language joins a list. */
export function kindList(t: Translate, language: string, counts: KindCounts): string {
  const parts = ADDED_KINDS.filter((kind) => counts[kind] > 0).map((kind) => t(`drop.kinds.${kind}`, { count: counts[kind] }));
  return new Intl.ListFormat(language, { type: 'conjunction' }).format(parts);
}

/** The Mods & packs tab that shows a kind. */
export const tabOf = (kind: AddedKind): ModsTab | undefined =>
  kind === 'mod' ? undefined : kind === 'resource_pack' ? 'resource_pack' : 'shader';

/** The instance a screen is about, which a drop goes to; null asks the player after the drop. A damaged
 *  instance takes nothing. */
export function contextInstance(route: Route, listing: InstanceListing | undefined): string | null {
  const id =
    route.name === 'home'
      ? pickId(listing, route.id)
      : route.name === 'mods' || route.name === 'instanceSettings'
        ? route.id
        : route.name === 'browse'
          ? (route.instance ?? null)
          : null;
  return id && listing?.instances.some((i) => i.id === id) ? id : null;
}

/** Skins has its own Import, and Manage is a server somewhere else: a drop of mods there would mean something
 *  else, so those screens take none. */
export const dropsAccepted = (route: Route) => route.name !== 'skins' && route.name !== 'manage';
