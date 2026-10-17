import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { ServerNews } from '../../ipc/schema';
import { qk, useServerNews } from '../../state/queries';
import { Megaphone } from '../../ui/Icon';
import { NewsDialog } from './NewsDialog';
import { eventLabel, isNew, latestLine, modpackSummary } from './newsRules';

/** The server's latest news in one quiet line: a megaphone, the post (an event with its time), and a blue
 *  square for news the player hasn't opened yet. Pressing it opens all the news. With no news, news off,
 *  offline or rate limited, nothing renders at all: news is a bonus, never a blocker. */
export function NewsLine({ invite, name, visible, onArt = false }: { invite: string | null; name: string; visible: boolean; onArt?: boolean }) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const news = useServerNews(invite, visible);
  const [open, setOpen] = useState(false);
  // The dialog joins the page on the first open and stays, so closing it hands focus back natively.
  const [opened, setOpened] = useState(false);
  const data = news.data;
  if (!invite || !data || data.posts.length === 0) return null;
  const now = new Date();
  const post = latestLine(data.posts, now)!;
  const when = eventLabel(post, now, t, i18n.language);
  const text = post.type === 'modpack' ? t('news.modpack.line', { summary: modpackSummary(post.modpack, t) }) : post.title;
  const line = when ? `${text} · ${when}` : text;
  const fresh = isNew(data.posts, data.seen, now);
  const openNews = () => {
    // The dialog starts from the page the line already holds, so it opens on the posts at once.
    qc.setQueryData<{ pages: (ServerNews | null)[]; pageParams: (string | null)[] }>(
      qk.newsAll(invite),
      { pages: [data], pageParams: [null] },
      { updatedAt: news.dataUpdatedAt },
    );
    setOpened(true);
    setOpen(true);
  };
  return (
    <>
      <button
        type="button"
        className={onArt ? 'news-line marquee-plate' : 'news-line'}
        aria-label={t(fresh ? 'news.lineLabelNew' : 'news.lineLabel', { line })}
        title={line}
        onClick={openNews}
      >
        <Megaphone aria-hidden className="news-line-icon" />
        <span className="news-line-text">
          {text}
          {when && <span className="news-line-when"> · {when}</span>}
        </span>
        {fresh && <span className="news-lamp" title={t('news.new')} />}
      </button>
      {/* Outside the card: inside it, the card's hover and focus rules would light its Play behind the backdrop. */}
      {opened && createPortal(<NewsDialog invite={invite} name={name} open={open} onClose={() => setOpen(false)} />, document.body)}
    </>
  );
}
