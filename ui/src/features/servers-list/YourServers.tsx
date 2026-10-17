import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { YourServerCard } from '../../ipc/schema';
import { useFreeServer, useUnfollow } from '../../state/queries';
import { navigate } from '../../state/route';
import { FreeServerCard } from '../free-server/FreeServerCard';
import { FreeServerEntry } from '../free-server/FreeServerEntry';
import { PublicServerCard } from './PublicServerCard';
import { useYours } from './yours';

/** The slot above Public servers: the free-server entry while free servers are on, then the player's own servers (their free
 *  server first, then Owner, Invited and followed), each with the public card's Play. Nothing while it loads or holds neither; signed out, one quiet line pointing to Accounts, only while accounts are on. */
export function YourServers() {
  const { t } = useTranslation();
  const { status, servers } = useYours();
  // The free server's page: null while free servers are off. While it is there the section stays, for its entry (and the
  // create dialog, which must outlive the entry once the server exists).
  const free = useFreeServer(status?.account != null).data;
  const unfollow = useUnfollow();
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
  // The free server went through its Delete: its card (and the dialog that held focus) is gone, so focus goes to the heading.
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
  if (!servers?.length && !free) return null;
  // The app already leaves the free server out of /servers; the same card is still never drawn twice.
  const others = (servers ?? []).filter((card) => !own || own.token === '' || card.token !== own.token);
  return (
    <section className="grid gap-4" aria-labelledby="your-servers">
      <h2 ref={heading} id="your-servers" tabIndex={-1} className="text-[1.125rem] leading-tight font-bold">
        {t('servers.yours.title')}
      </h2>
      {free && <FreeServerEntry page={free} />}
      {(own || others.length > 0) && (
        <ul className="servers-grid">
          {own && (
            <li key="free-server" className="grid">
              <FreeServerCard server={own} onDeleted={() => (deleted.current = true)} />
            </li>
          )}
          {others.map((card) => (
            <li key={card.token} className="grid">
              <PublicServerCard
                card={card}
                tag={tagOf(card)}
                follow={card.relation === 'followed'}
                menu={card.relation === 'followed' && card.invite ? { onUnfollow: () => onUnfollow(card) } : undefined}
              />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** The public list's heading (`ServersScreen`), where focus goes when this section leaves. */
export const PUBLIC_HEADING = 'public-servers';

const tagOf = (card: YourServerCard) => (card.relation === 'followed' ? undefined : card.relation);
