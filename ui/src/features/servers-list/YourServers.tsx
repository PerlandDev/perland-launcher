import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { YourServerCard } from '../../ipc/schema';
import { useUnfollow } from '../../state/queries';
import { navigate } from '../../state/route';
import { PublicServerCard } from './PublicServerCard';
import { useYours } from './yours';

/** The slot above Public servers: the player's own servers (Owner, Invited, then followed), each with the public card's Play.
 *  Nothing while it loads or holds none; signed out, one quiet line pointing to Accounts, only while accounts are on. */
export function YourServers() {
  const { t } = useTranslation();
  const { status, servers } = useYours();
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
  if (!servers?.length) return null;
  return (
    <section className="grid gap-4" aria-labelledby="your-servers">
      <h2 ref={heading} id="your-servers" tabIndex={-1} className="text-[1.125rem] leading-tight font-bold">
        {t('servers.yours.title')}
      </h2>
      <ul className="servers-grid">
        {servers.map((card) => (
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
    </section>
  );
}

/** The public list's heading (`ServersScreen`), where focus goes when this section leaves. */
export const PUBLIC_HEADING = 'public-servers';

const tagOf = (card: YourServerCard) => (card.relation === 'followed' ? undefined : card.relation);
