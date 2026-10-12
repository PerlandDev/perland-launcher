import type { TFunction } from 'i18next';
import type { Instance, LoaderKind, ServerAddress } from '../../ipc/schema';

const NAMES: Record<LoaderKind, string> = { fabric: 'Fabric', quilt: 'Quilt', forge: 'Forge', neoforge: 'NeoForge' };

export const isPack = (instance: Instance) => instance.source.kind !== 'none';

export const isServer = (instance: Instance) => instance.source.kind === 'perland';

/** The saved invite of a server instance. */
export const inviteOf = (instance: Instance): string | null =>
  instance.source.kind === 'perland' ? instance.source.invite : null;

/** "play.example.com:25566", with brackets around an IPv6 literal. */
export function addressText(address: ServerAddress): string {
  return address.host.includes(':') ? `[${address.host}]:${address.port}` : `${address.host}:${address.port}`;
}

export const loaderName = (kind: LoaderKind) => NAMES[kind];

/** "Minecraft 1.20.1 · Fabric 0.16.14" or "Minecraft 26.3 · Vanilla". */
export function versionLine(t: TFunction, instance: Instance): string {
  const mc = t('home.minecraftLine', { version: instance.game.minecraft });
  const loader = instance.game.loader;
  const second = loader ? t('home.loaderLine', { loader: loaderName(loader.kind), version: loader.version }) : t('home.vanilla');
  return `${mc} · ${second}`;
}

/** "+3 added · 2 updated · 1 removed", or a line for a new version with the same mods. */
export function changeLine(t: TFunction, counts: { added: number; updated: number; removed: number }): string {
  const parts = [
    counts.added > 0 ? t('update.added', { count: counts.added }) : null,
    counts.updated > 0 ? t('update.updated', { count: counts.updated }) : null,
    counts.removed > 0 ? t('update.removed', { count: counts.removed }) : null,
  ].filter((part): part is string => part !== null);
  return parts.length > 0 ? parts.join(' · ') : t('update.sameFiles');
}
