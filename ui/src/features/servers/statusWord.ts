import type { TFunction } from 'i18next';
import type { ServerPlayers, ServerStatus } from '../../ipc/schema';

/** The status in words; a dead link or no data is "Unknown". */
export function statusWord(t: TFunction, status: ServerStatus | null, players: ServerPlayers | null, gone: boolean): string {
  if (gone || !status) return t('server.unknown');
  switch (status) {
    case 'online':
      if (!players) return t('server.online');
      return players.max !== null
        ? t('server.onlinePlayers', { online: players.online, max: players.max })
        : t('server.onlineCount', { online: players.online });
    case 'starting':
      return t('server.starting');
    case 'sleeping':
      return t('server.sleeping');
    case 'offline':
      return t('server.offline');
  }
}
