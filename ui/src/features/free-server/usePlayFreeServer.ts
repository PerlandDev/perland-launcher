import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { commands } from '../../ipc/commands';
import { isLauncherError } from '../../ipc/errors';
import { qk, useFreeServerPlay } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';

/** Play on the player's own free server, as every Play does: `free_server_play` starts it when it is off, joins it and answers
 *  the instance; then the game starts with join and its Home opens. `onStarted` runs once the game is on its way. Errors are
 *  toasts, except a game already running, whose Home is where the player wants to be. */
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
  const run = () => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    void (async () => {
      let id: string | null = null;
      try {
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
