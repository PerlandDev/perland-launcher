// The UI's copy of launcher-core's `browse::compat` rules, for the "Install to" picker. The core
// re-checks every plan, so a mismatch here only disables an option.
import type { ContentKind, Instance, LoaderKind, VersionInfo } from '../../ipc/schema';

export function loaderTags(loader: LoaderKind, minecraft: string): string[] {
  if (loader === 'quilt') return ['quilt', 'fabric'];
  if (loader === 'neoforge' && minecraft === '1.20.1') return ['neoforge', 'forge'];
  return [loader];
}

export function fitsVersion(v: VersionInfo, kind: ContentKind, instance: Instance): boolean {
  const { minecraft, loader } = instance.game;
  if (!v.gameVersions.includes(minecraft)) return false;
  switch (kind) {
    case 'mod':
      return loader !== null && loaderTags(loader.kind, minecraft).some((tag) => v.loaders.includes(tag));
    case 'shader':
      return loader !== null && v.loaders.includes('iris');
    case 'resource_pack':
      return true;
    case 'modpack':
      return false;
  }
}

export type Fit = { ok: true } | { ok: false; reason: 'needsLoader' | 'noVersion' };

export function fitFor(versions: VersionInfo[], kind: ContentKind, instance: Instance): Fit {
  if ((kind === 'mod' || kind === 'shader') && instance.game.loader === null) return { ok: false, reason: 'needsLoader' };
  return versions.some((v) => fitsVersion(v, kind, instance)) ? { ok: true } : { ok: false, reason: 'noVersion' };
}

const stability = (type: string) => (type === 'release' ? 0 : type === 'beta' ? 1 : 2);

/** Releases first, then betas, then alphas; the newest within each. */
export function pickDefault(versions: VersionInfo[]): VersionInfo | undefined {
  return [...versions].sort((a, b) => stability(a.versionType) - stability(b.versionType) || b.date.localeCompare(a.date))[0];
}
