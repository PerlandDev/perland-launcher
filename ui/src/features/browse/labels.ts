import type { i18n as I18n } from 'i18next';
import type { LoaderKind, VersionInfo } from '../../ipc/schema';
import { loaderName } from '../home/labels';

/** "Fabric 1.20.1", or "Minecraft 1.21.1" without a loader. */
export const gameLabel = (minecraft: string, loader: LoaderKind | null) =>
  loader ? `${loaderName(loader)} ${minecraft}` : `Minecraft ${minecraft}`;

/** A translated category, or the slug made readable ("game-mechanics" → "Game mechanics"). */
export function categoryLabel(i18n: I18n, slug: string): string {
  const key = `categories.${slug}`;
  if (i18n.exists(key)) return i18n.t(key);
  const words = slug.replace(/[-_]+/g, ' ').trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** "mc1.20.1-0.5.11 · 1.20.1" (the newest few game versions it supports). */
export const versionLabel = (v: VersionInfo) => `${v.number} · ${v.gameVersions.slice(-3).join(', ')}`;
