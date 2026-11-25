import { describe, expect, it } from 'vitest';
import type { DroppedFile, InstanceListing } from '../../ipc/schema';
import type { Route } from '../../state/route';
import { contextInstance, dropsAccepted, kindList, planDrop, skippedUpFront } from './dropPlan';

const f = (name: string, kind: DroppedFile['kind']): DroppedFile => ({ path: `C:/d/${name}`, name, kind });
const jar = (n: string) => f(`${n}.jar`, 'mod');

describe('planDrop', () => {
  it('counts what a modded instance will take, and what it will skip', () => {
    const plan = planDrop([jar('a'), jar('b'), f('BSL.zip', 'shader_pack'), f('F.zip', 'resource_pack'), f('notes.txt', 'other'), f('p.zip', 'unknown_zip')], true);
    expect(plan).toEqual({ modpack: null, adds: { mod: 2, resource_pack: 1, shader_pack: 1 }, needsLoader: 0, other: 2 });
  });

  it('skips mods and shader packs on a vanilla instance, keeping resource packs', () => {
    const plan = planDrop([jar('a'), f('BSL.zip', 'shader_pack'), f('F.zip', 'resource_pack')], false);
    expect(plan.adds).toEqual({ mod: 0, resource_pack: 1, shader_pack: 0 });
    expect(plan.needsLoader).toBe(2);
  });

  it('counts everything as added while the instance is still to be chosen', () => {
    expect(planDrop([jar('a'), f('BSL.zip', 'shader_pack')], null).adds).toEqual({ mod: 1, resource_pack: 0, shader_pack: 1 });
  });

  it('imports a modpack dropped on its own, and skips one among other files', () => {
    const sky = f('Sky.mrpack', 'modpack');
    expect(planDrop([sky], true).modpack).toEqual(sky);
    const mixed = planDrop([sky, jar('a')], true);
    expect(mixed.modpack).toBeNull();
    expect(mixed.other).toBe(1);
    expect(mixed.adds.mod).toBe(1);
  });
});

describe('skippedUpFront', () => {
  it('names what no instance would take, with the reason the core gives', () => {
    expect(skippedUpFront([jar('a'), f('notes.txt', 'other'), f('p.zip', 'unknown_zip'), f('Sky.mrpack', 'modpack')])).toEqual([
      { name: 'notes.txt', kind: 'other', reason: 'not_content' },
      { name: 'p.zip', kind: 'unknown_zip', reason: 'not_a_pack' },
      { name: 'Sky.mrpack', kind: 'modpack', reason: 'modpack' },
    ]);
  });
});

describe('kindList', () => {
  it('joins the kinds in the language, leaving out the empty ones', () => {
    const t = (key: string, opts?: { count?: number }) => `${opts?.count} ${key.split('.').pop()}`;
    expect(kindList(t, 'en', { mod: 2, resource_pack: 0, shader_pack: 1 })).toBe('2 mod and 1 shader_pack');
    expect(kindList(t, 'en', { mod: 2, resource_pack: 1, shader_pack: 1 })).toBe('2 mod, 1 resource_pack, and 1 shader_pack');
    expect(kindList(t, 'es', { mod: 2, resource_pack: 1, shader_pack: 1 })).toBe('2 mod, 1 resource_pack y 1 shader_pack');
    expect(kindList(t, 'en', { mod: 0, resource_pack: 0, shader_pack: 0 })).toBe('');
  });
});

describe('where a drop goes', () => {
  const listing = {
    instances: [{ id: 'asalith-fields' }, { id: 'creative-flat' }],
    damaged: [{ id: 'old-pack' }],
  } as unknown as InstanceListing;

  it('goes to the instance the screen is about', () => {
    const cases: [Route, string | null][] = [
      [{ name: 'home', id: 'creative-flat' }, 'creative-flat'],
      [{ name: 'mods', id: 'creative-flat', tab: 'shader' }, 'creative-flat'],
      [{ name: 'instanceSettings', id: 'creative-flat' }, 'creative-flat'],
      [{ name: 'browse', kind: 'mod', instance: 'creative-flat' }, 'creative-flat'],
      // Home with no id shows the first instance (no last one is remembered in tests).
      [{ name: 'home', id: null }, 'asalith-fields'],
      // A damaged instance takes nothing: the player is asked.
      [{ name: 'home', id: 'old-pack' }, null],
      [{ name: 'browse', kind: 'mod' }, null],
      [{ name: 'servers' }, null],
      [{ name: 'settings' }, null],
      [{ name: 'newInstance' }, null],
      [{ name: 'mods', id: 'gone' }, null],
    ];
    for (const [route, id] of cases) expect(contextInstance(route, listing), JSON.stringify(route)).toBe(id);
  });

  it('is turned off where a drop means something else or nothing', () => {
    expect(dropsAccepted({ name: 'skins' })).toBe(false);
    expect(dropsAccepted({ name: 'manage', tenantId: 't', stack: 's' })).toBe(false);
    expect(dropsAccepted({ name: 'home', id: null })).toBe(true);
    expect(dropsAccepted({ name: 'settings' })).toBe(true);
  });
});
