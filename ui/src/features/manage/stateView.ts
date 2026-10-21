import type { TFunction } from 'i18next';
import type { ManageState, ServerStatus } from '../../ipc/schema';

/** A managed server's state in words, and the LED it rides: lime running, blue while it changes, a switched-off lamp asleep,
 *  coral stopped. Shared by the Manage screen and the Your servers card of a server only Manage knows, so both say it alike.
 *  `online` is the player count when known; `crashed` turns Stopped into Crashed once the diagnosis says the last run failed. */
export function manageStateView(t: TFunction, state: ManageState, online: number | null, crashed = false): { word: string; led: ServerStatus | null } {
  switch (state) {
    case 'running':
      return { word: online === null ? t('manage.state.runningNoCount') : t('manage.state.running', { count: online }), led: 'online' };
    case 'asleep':
      return { word: t('manage.state.asleep'), led: 'sleeping' };
    case 'starting':
    case 'stopping':
      return { word: t(`manage.state.${state}`), led: 'starting' };
    case 'stopped':
      return { word: crashed ? t('manage.state.crashed') : t('manage.state.stopped'), led: 'offline' };
    default:
      return { word: t('manage.state.unknown'), led: null };
  }
}
