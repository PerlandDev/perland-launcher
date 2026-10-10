import type { TFunction } from 'i18next';
import type { Instance, LoaderKind } from '../../ipc/schema';

const NAMES: Record<LoaderKind, string> = { fabric: 'Fabric', quilt: 'Quilt', forge: 'Forge', neoforge: 'NeoForge' };

export const loaderName = (kind: LoaderKind) => NAMES[kind];

/** "Minecraft 1.20.1 · Fabric 0.16.14" or "Minecraft 26.3 · Vanilla". */
export function versionLine(t: TFunction, instance: Instance): string {
  const mc = t('home.minecraftLine', { version: instance.game.minecraft });
  const loader = instance.game.loader;
  const second = loader ? t('home.loaderLine', { loader: loaderName(loader.kind), version: loader.version }) : t('home.vanilla');
  return `${mc} · ${second}`;
}
