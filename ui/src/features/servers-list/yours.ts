import { usePerlandStatus, useYourServers } from '../../state/queries';

/** The Perland status and, while signed in, the player's Your servers (undefined while it loads, signed out or accounts off). */
export function useYours() {
  const status = usePerlandStatus();
  const signedIn = status.data?.account != null;
  const list = useYourServers(signedIn);
  return { status: status.data, servers: signedIn ? list.data?.servers : undefined };
}

/** The invite of each followed server by token, so the public list can give the same card Unfollow. */
export function useFollowed(): Map<string, string> {
  const { servers } = useYours();
  const followed = new Map<string, string>();
  for (const s of servers ?? []) if (s.relation === 'followed' && s.invite) followed.set(s.token, s.invite);
  return followed;
}
