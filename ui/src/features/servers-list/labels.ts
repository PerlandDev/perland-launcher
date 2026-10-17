import type { TFunction } from 'i18next';
import type { LoaderKind, PublicServerCard } from '../../ipc/schema';
import { loaderName } from '../home/labels';

const LOADERS: readonly string[] = ['fabric', 'quilt', 'forge', 'neoforge'];

/** A card's status in the list's own words: "7 / 20 playing", "Sleeping, Play wakes it", "Starting". */
export function listStatusWord(t: TFunction, card: Pick<PublicServerCard, 'status' | 'players'>): string {
  switch (card.status) {
    case 'online':
      if (!card.players) return t('server.online');
      return card.players.max !== null
        ? t('servers.playing', { online: card.players.online, max: card.players.max })
        : t('servers.playingCount', { online: card.players.online });
    case 'starting':
      return t('servers.starting');
    case 'sleeping':
      return t('servers.sleeping');
    case 'offline':
      return t('server.offline');
  }
}

/** One line for the game: the modpack's name and version, else the loader or Vanilla and the Minecraft version. */
export function gameLine(t: TFunction, card: Pick<PublicServerCard, 'pack' | 'game'>): string {
  if (card.pack) return [card.pack.name, card.pack.version].filter(Boolean).join(' ');
  const loader = card.game.loader;
  const name = !loader ? t('servers.vanilla') : LOADERS.includes(loader) ? loaderName(loader as LoaderKind) : loader;
  return [name, card.game.version].filter(Boolean).join(' ');
}

/** A category id ("modded-tech") in the player's words; an id the launcher doesn't know shows as itself. */
export const categoryName = (t: TFunction, id: string) => t(`servers.category.${id}`, { defaultValue: id });

export const languageName = (t: TFunction, id: string) => t(`servers.language.${id}`, { defaultValue: id });

export const regionName = (t: TFunction, id: string) => t(`servers.region.${id}`, { defaultValue: id.toUpperCase() });
