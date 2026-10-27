import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { commands } from '../../ipc/commands';
import { isLauncherError } from '../../ipc/errors';
import { qk, useFreeServerPlay } from '../../state/queries';
import { requireAccount } from '../../state/requireAccount';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';

/** Play on the player's own free server, as every Play does: `free_server_play` starts it when it is off, joins it and answers
 *  the instance; then the game starts with join and its Home opens. `onStarted` runs once the game is on its way. Without an
 *  account that can play, the sign-in prompt opens first and nothing is started. Errors are toasts, except a game already
 *  running, whose Home is where the player wants to be. */
export function usePlayFreeServer(onStarted?: () => void) {
  const qc = useQueryClient();
  const freePlay = useFreeServerPlay({ inline: true });
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  /** Resolves once the attempt is over (it never rejects: errors are toasts). */
  const run = (): Promise<void> => {
    if (running.current) return Promise.resolve();
    running.current = true;
    setBusy(true);
    return (async () => {
      let id: string | null = null;
      try {
        // Without an account that can play, the sign-in prompt opens and the server is left asleep.
        if (!(await requireAccount())) return;
        id = await freePlay.mutateAsync();
        await qc.invalidateQueries({ queryKey: qk.instances });
        await commands.play(id, true);
        onStarted?.();
        navigate({ name: 'home', id });
      } catch (error) {
        if (id && isLauncherError(error) && error.code === 'already_running') {
          onStarted?.();
          navigate({ name: 'home', id });
        } else pushError(error);
      } finally {
        running.current = false;
        if (mounted.current) setBusy(false);
      }
    })();
  };
  return { busy, run };
}
