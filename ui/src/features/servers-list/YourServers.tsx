import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { YourServerCard } from '../../ipc/schema';
import { useCreatableTeams, useFreeServer, useUnfollow } from '../../state/queries';
import { navigate } from '../../state/route';
import { FreeServerCard } from '../free-server/FreeServerCard';
import { FreeServerEntry } from '../free-server/FreeServerEntry';
import { ManagedServerCard } from './ManagedServerCard';
import { PublicServerCard } from './PublicServerCard';
import { useManageItem, useManageKey, useYours } from './yours';

/** The slot above Public servers: the free-server entry while free servers are on, then the player's own servers: their free
 *  server first, then the servers they run from the launcher (owner cards with Manage beside Play, then the ones only Manage
 *  knows), then Owner, Invited and followed as listed, each with the public card's Play, and Manage in Perland on owned cards
 *  without Manage while a panel is configured. Nothing while it loads or holds neither; signed out, one quiet line pointing to Accounts, only while accounts are on. */
export function YourServers() {
  const { t } = useTranslation();
  const { status, servers, managed } = useYours();
  // The free server's page: null while free servers are off. While it is there the section stays, for its entry (and the
  // create dialog, which must outlive the entry once the server exists).
  const free = useFreeServer(status?.account != null).data;
  // The teams the player may create a server on: with any, the entry is "Create a server" with Where.
  const teams = useCreatableTeams(status?.account != null).data ?? [];
  const unfollow = useUnfollow();
  const manage = useManageItem();
  const manageKey = useManageKey();
  const heading = useRef<HTMLHeadingElement>(null);
  // The token of a card whose Unfollow went through, until the card leaves; `unfollowed` re-runs the check below.
  const leaving = useRef<string | null>(null);
  const [unfollowed, setUnfollowed] = useState(0);
  const onUnfollow = (card: YourServerCard) =>
    unfollow.mutate(card.invite as string, {
      onSuccess: () => {
        leaving.current = card.token;
        setUnfollowed((n) => n + 1);
      },
    });
  // The card (and its menu, which held focus) is gone: focus would fall to the page, so it goes to this section's
  // heading, or to the public list's when the section went with it. Never taken from where the player moved it.
  useEffect(() => {
    const token = leaving.current;
    if (token === null || servers?.some((s) => s.token === token)) return;
    leaving.current = null;
    if (document.activeElement !== null && document.activeElement !== document.body) return;
    (heading.current ?? document.getElementById(PUBLIC_HEADING))?.focus();
  }, [servers, unfollowed]);
  // The player confirmed the free server's Delete (cleared when nothing was deleted): once its card (and the dialog that held
  // focus) is gone, focus goes to the heading. Set at Confirm, so it holds however the delete and the page's refetch interleave.
  const deleted = useRef(false);
  const own = free?.server ?? null;
  useEffect(() => {
    if (!deleted.current || own) return;
    deleted.current = false;
    if (document.activeElement !== null && document.activeElement !== document.body) return;
    heading.current?.focus();
  }, [own]);
  if (!status?.enabled) return null;
  if (!status.account) {
    return (
      <p className="text-sm text-ink-dim">
        {t('servers.yours.nudge')}{' '}
        <button type="button" className="text-blue-ink underline underline-offset-4" onClick={() => navigate({ name: 'accounts' })}>
          {t('servers.yours.signIn')}
        </button>
      </p>
    );
  }
  if (!servers?.length && !managed?.length && !free && teams.length === 0) return null;
  // The app already leaves the free server out of /servers; the same card is still never drawn twice.
  const listed = (servers ?? []).filter((card) => !own || own.token === '' || card.token !== own.token);
  // The servers the player runs come first: owner cards with Manage, then the ones only Manage knows, then the rest as listed.
  const run = listed.filter((card) => card.manage !== null);
  const others = listed.filter((card) => card.manage === null);
  const runOnly = managed ?? [];
  const card = (item: YourServerCard) => (
    <li key={item.token} className="grid">
      <PublicServerCard
        card={item}
        tag={tagOf(item)}
        follow={item.relation === 'followed'}
        menu={item.relation === 'followed' && item.invite ? { onUnfollow: () => onUnfollow(item) } : undefined}
        primary={manageKey(item.manage)}
        // With Manage on the card, More in Perland is on the Manage screen.
        menuItems={item.relation === 'owner' && !item.manage ? manage(item.name) : undefined}
      />
    </li>
  );
  return (
    <section className="grid gap-4" aria-labelledby="your-servers">
      <h2 ref={heading} id="your-servers" tabIndex={-1} className="text-[1.125rem] leading-tight font-bold">
        {t('servers.yours.title')}
      </h2>
      {(free || teams.length > 0) && <FreeServerEntry page={free ?? null} teams={teams} />}
      {(own || listed.length > 0 || runOnly.length > 0) && (
        <ul className="servers-grid">
          {own && (
            <li key="free-server" className="grid">
              <FreeServerCard server={own} onDeleting={(going) => (deleted.current = going)} />
            </li>
          )}
          {run.map(card)}
          {runOnly.map((server) => (
            <li key={`managed:${server.tenantId}/${server.stack}`} className="grid">
              <ManagedServerCard server={server} />
            </li>
          ))}
          {others.map(card)}
        </ul>
      )}
    </section>
  );
}

/** The public list's heading (`ServersScreen`), where focus goes when this section leaves. */
export const PUBLIC_HEADING = 'public-servers';

const tagOf = (card: YourServerCard) => (card.relation === 'followed' ? undefined : card.relation);
