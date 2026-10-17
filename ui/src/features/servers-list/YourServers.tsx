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
      <h2 id="your-servers" className="text-[1.125rem] leading-tight font-bold">
        {t('servers.yours.title')}
      </h2>
      <ul className="servers-grid">
        {servers.map((card) => (
          <li key={card.token} className="grid">
            <PublicServerCard
              card={card}
              tag={tagOf(card)}
              follow={card.relation === 'followed'}
              menu={card.relation === 'followed' && card.invite ? { onUnfollow: () => unfollow.mutate(card.invite as string) } : undefined}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}

const tagOf = (card: YourServerCard) => (card.relation === 'followed' ? undefined : card.relation);
