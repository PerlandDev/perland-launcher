import { useContentInstalling } from './contentProgress';
import { useActiveIds } from './runStore';

/** Restart now waits while a game runs or a Browse install is in flight, as the core's
 *  `update_blocked` does (an instance install is a run, so it counts as a game here). */
export function useUpdateBusy(): boolean {
  const games = useActiveIds().length > 0;
  const installs = useContentInstalling();
  return games || installs;
}
