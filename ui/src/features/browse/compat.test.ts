import { describe, expect, it } from 'vitest';
import type { Instance, VersionInfo } from '../../ipc/schema';
import { fitFor, fitsNewInstance, fitsVersion, loaderTags, pickDefault } from './compat';

const instance = (minecraft: string, loader: Instance['game']['loader']): Instance => ({
  id: 'i',
  name: 'I',
  source: { kind: 'none' },
  game: { minecraft, loader },
  updatePolicy: 'ask',
  optionalChoices: {},
  java: { path: null, memoryMb: 4096 },
  server: null,
});
const version = (id: string, type: string, date: string, mc: string[], loaders: string[]): VersionInfo => ({
  id,
  name: id,
  number: id,
  versionType: type,
  gameVersions: mc,
  loaders,
  date,
  size: 1,
});
const fabric = instance('1.20.1', { kind: 'fabric', version: '0.16.14' });
const quilt = instance('1.20.1', { kind: 'quilt', version: '0.24.0' });
const vanilla = instance('1.20.1', null);

describe('compat', () => {
  it('follows the core loader rules', () => {
    expect(loaderTags('quilt', '1.20.1')).toEqual(['quilt', 'fabric']);
    expect(loaderTags('neoforge', '1.20.1')).toEqual(['neoforge', 'forge']);
    expect(loaderTags('neoforge', '1.21.1')).toEqual(['neoforge']);
  });

  it('fits mods, shaders and resource packs', () => {
    const fabricMod = version('a', 'release', '2026-01-01', ['1.20.1'], ['fabric']);
    expect(fitsVersion(fabricMod, 'mod', quilt)).toBe(true);
    expect(fitsVersion(fabricMod, 'mod', vanilla)).toBe(false);
    expect(fitsVersion(version('s', 'release', 'd', ['1.20.1'], ['iris']), 'shader', fabric)).toBe(true);
    expect(fitsVersion(version('s', 'release', 'd', ['1.20.1'], ['iris']), 'shader', vanilla)).toBe(false);
    expect(fitsVersion(version('r', 'release', 'd', ['1.20.1'], ['minecraft']), 'resource_pack', vanilla)).toBe(true);
    expect(fitFor([fabricMod], 'mod', vanilla)).toEqual({ ok: false, reason: 'needsLoader' });
    expect(fitFor([fabricMod], 'mod', instance('1.21.1', { kind: 'fabric', version: '1' }))).toEqual({ ok: false, reason: 'noVersion' });
    expect(fitFor([fabricMod], 'mod', fabric)).toEqual({ ok: true });
  });

  it('mirrors the core rules for a version that can start a new instance', () => {
    const v = (mc: string[], loaders: string[]) => version('n', 'release', 'd', mc, loaders);
    expect(fitsNewInstance(v(['1.21.1'], ['fabric']), 'mod')).toBe(true);
    expect(fitsNewInstance(v(['1.21.1'], ['forge']), 'mod')).toBe(true);
    expect(fitsNewInstance(v(['1.21.1'], ['neoforge']), 'mod')).toBe(true);
    expect(fitsNewInstance(v(['1.21.1'], ['quilt']), 'mod')).toBe(true);
    // A server plugin build has no loader the launcher can make.
    expect(fitsNewInstance(v(['1.21.1'], ['bukkit', 'paper', 'spigot']), 'mod')).toBe(false);
    // Snapshots only: no release to make the instance on.
    expect(fitsNewInstance(v(['24w14a', '1.21-pre1'], ['fabric']), 'mod')).toBe(false);
    expect(fitsNewInstance(v(['1.20', '24w14a'], ['fabric']), 'mod')).toBe(true);
    expect(fitsNewInstance(v(['1.21.1'], ['iris']), 'shader')).toBe(true);
    expect(fitsNewInstance(v(['1.21.1'], ['fabric']), 'shader')).toBe(false);
    expect(fitsNewInstance(v(['1.21.1'], ['minecraft']), 'resource_pack')).toBe(true);
    expect(fitsNewInstance(v(['24w14a'], ['minecraft']), 'resource_pack')).toBe(false);
    expect(fitsNewInstance(v(['1.21.1'], ['fabric']), 'modpack')).toBe(false);
  });

  it('defaults to the newest release, then beta, then alpha', () => {
    const list = [
      version('beta-new', 'beta', '2026-03-01', [], []),
      version('rel-old', 'release', '2026-01-01', [], []),
      version('rel-new', 'release', '2026-02-01', [], []),
    ];
    expect(pickDefault(list)?.id).toBe('rel-new');
    expect(pickDefault(list.slice(0, 1))?.id).toBe('beta-new');
    expect(pickDefault([])).toBeUndefined();
  });
});
