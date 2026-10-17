import { useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { inviteCanonical } from '../../ipc/inviteLink';
import type { Instance, YourServerCard } from '../../ipc/schema';
import { qk, usePerlandStatus, useYourServers } from '../../state/queries';
import { inviteOf } from '../home/labels';

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

const RELATION_ORDER = { owner: 0, invited: 1, followed: 2 } as const;

/** The loader a server's instance gets, as the core decides it: none without a modpack, and `vanilla` is none. */
function cardLoader(card: YourServerCard): string | null {
  const loader = card.pack ? (card.game.loader?.trim().toLowerCase() ?? '') : '';
  return loader && loader !== 'vanilla' ? loader : null;
}

/** The player's servers `instance` could link to without moving to another game: joinable from the launcher, on the
 *  instance's Minecraft version and loader, and not the server it already links to. Owned first, then invited, then
 *  followed, each in the list's own order. */
export function linkSuggestions(servers: YourServerCard[], instance: Instance): YourServerCard[] {
  const own = inviteOf(instance);
  const linked = own ? (inviteCanonical(own) ?? own) : null;
  return servers
    .filter(
      (card) =>
        card.invite !== null &&
        card.joinable &&
        card.game.version === instance.game.minecraft &&
        cardLoader(card) === (instance.game.loader?.kind ?? null) &&
        (inviteCanonical(card.invite) ?? card.invite) !== linked,
    )
    .sort((a, b) => RELATION_ORDER[a.relation] - RELATION_ORDER[b.relation]);
}

/** `linkSuggestions` from the signed-in player's Your servers; none while signed out or loading. */
export function useLinkSuggestions(instance: Instance | null): YourServerCard[] {
  const { signedIn, list } = useYourList();
  if (!instance || !signedIn) return [];
  return linkSuggestions(list.data?.servers ?? [], instance);
}
