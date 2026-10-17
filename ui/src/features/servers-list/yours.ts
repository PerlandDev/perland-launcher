import { useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { qk, usePerlandStatus, useYourServers } from '../../state/queries';

function useYourList() {
  const status = usePerlandStatus();
  const signedIn = status.data?.account != null;
  const list = useYourServers(signedIn);
  return { status: status.data, signedIn, list };
}

/** The Perland status and, while signed in, the player's Your servers (undefined while it loads, signed out or accounts off).
 *  Signed in, a null list means the Engine ended the session (a 401, which the backend forgets) or closed the feature, so
 *  the status is read again and a forgotten session reads as signed out: once per such answer, not on every refetch. Call it
 *  from one component of a screen (Your servers), so the status is read again once. */
export function useYours() {
  const qc = useQueryClient();
  const { status, signedIn, list } = useYourList();
  const nullWhileSignedIn = signedIn && list.isSuccess && list.data === null;
  useEffect(() => {
    if (nullWhileSignedIn) void qc.invalidateQueries({ queryKey: qk.perland });
  }, [nullWhileSignedIn, qc]);
  return { status, servers: signedIn ? list.data?.servers : undefined };
}

/** The invite of each followed server by token, so the public list can give the same card Unfollow. */
export function useFollowed(): Map<string, string> {
  const { signedIn, list } = useYourList();
  const followed = new Map<string, string>();
  for (const s of (signedIn ? list.data?.servers : undefined) ?? []) if (s.relation === 'followed' && s.invite) followed.set(s.token, s.invite);
  return followed;
}
