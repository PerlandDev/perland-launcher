import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { NewsChangesView, NewsPostView } from '../../ipc/schema';
import { qk, useNewsImage, useServerNewsPages } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { Calendar } from '../../ui/Icon';
import { eventLabel, isNew, modpackSummary, newestCreatedAt, parseNewsTime } from './newsRules';

/** Names a modpack part lists before "and N more". */
const NAMES_SHOWN = 10;

/** All of a server's news, newest first, 20 a page with Show more. Opening it marks the news as seen, so the
 *  line's dot goes away. Nothing in it is lit: Close is ghost and Show more is raised. */
export function NewsDialog({ invite, name, open, onClose }: { invite: string; name: string; open: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const pages = useServerNewsPages(invite, open);
  const loaded = pages.data?.pages.filter((page) => page !== null) ?? [];
  const posts = loaded.flatMap((page) => page.posts);
  const first = loaded[0] ?? null;
  // The marker the dialog opened with, so posts stay marked New while it is open.
  const seen = first?.seen ?? null;
  const newest = first ? newestCreatedAt(first.posts) : null;

  // Once per open: the marker moves to the newest post shown, and the line reads again so its dot clears.
  const marked = useRef<string | null>(null);
  useEffect(() => {
    if (!open) {
      marked.current = null;
      return;
    }
    if (newest === null || marked.current === newest) return;
    marked.current = newest;
    void commands
      .serverNewsOpened(invite, newest)
      .then(() => qc.invalidateQueries({ queryKey: qk.news(invite) }))
      .catch(() => {});
  }, [open, newest, invite, qc]);

  const now = new Date();
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t('news.title', { name })}
      actions={
        <Button variant="ghost" onClick={onClose}>
          {t('news.close')}
        </Button>
      }
    >
      {/* The list scrolls on its own, so Close stays in view; it takes focus first, at the top. */}
      <section className="news-list" tabIndex={0} aria-label={t('news.label')}>
        {posts.map((post) => (
          <NewsPost key={post.id} invite={invite} post={post} now={now} fresh={isNew([post], seen, now)} />
        ))}
        {pages.isFetchNextPageError ? (
          <div className="news-more" role="status">
            <p>{t('news.loadFailed')}</p>
            <Button onClick={() => void pages.fetchNextPage()}>{t('news.retry')}</Button>
          </div>
        ) : (
          pages.hasNextPage && (
            <div className="news-more">
              <Button aria-disabled={pages.isFetchingNextPage || undefined} onClick={() => !pages.isFetchingNextPage && void pages.fetchNextPage()}>
                {t('news.showMore')}
              </Button>
            </div>
          )
        )}
      </section>
    </Dialog>
  );
}

/** One post: its title, when (an event's time, else the day it was posted), its picture, and its text. */
function NewsPost({ invite, post, now, fresh }: { invite: string; post: NewsPostView; now: Date; fresh: boolean }) {
  const { t, i18n } = useTranslation();
  const event = eventLabel(post, now, t, i18n.language);
  const created = parseNewsTime(post.createdAt);
  // A post whose time does not parse goes without its Posted line.
  const when = event ?? (created === null ? null : t('news.posted', { date: new Intl.DateTimeFormat(i18n.language, { month: 'short', day: 'numeric' }).format(created) }));
  return (
    <article className="news-post">
      <h3 className="news-post-title">{post.type === 'modpack' ? t('news.modpack.title') : post.title}</h3>
      {(when || fresh) && (
        <p className="news-post-when">
          {event && <Calendar aria-hidden />}
          {when && <span>{when}</span>}
          {fresh && <span className="news-post-new">{t('news.new')}</span>}
        </p>
      )}
      <NewsPicture invite={invite} post={post} />
      {post.type === 'modpack' ? (
        <ModpackChanges changes={post.modpack} />
      ) : (
        // Plain text with its line breaks: never parsed as HTML or Markdown.
        post.body && <p className="news-post-body">{post.body}</p>
      )}
    </article>
  );
}

/** The post's picture at its own size, shrunk to fit and never blown up. */
function NewsPicture({ invite, post }: { invite: string; post: NewsPostView }) {
  const image = useNewsImage(invite, post.id, post.imageVersion);
  if (!image.data) return null;
  return <img className="news-picture" src={image.data} alt="" draggable={false} />;
}

/** A modpack post in the player's language: the counts, then up to 10 names a part. */
function ModpackChanges({ changes }: { changes: NewsChangesView | null }) {
  const { t } = useTranslation();
  const parts = changes
    ? ([
        ['news.modpack.addedList', changes.added],
        ['news.modpack.removedList', changes.removed],
        ['news.modpack.updatedList', changes.updated],
      ] as const).filter(([, list]) => list.count > 0 && list.names.length > 0)
    : [];
  return (
    <>
      <p className="news-post-body">{modpackSummary(changes, t)}</p>
      {parts.length > 0 && (
        <dl className="news-mods">
          {parts.map(([label, list]) => {
            const shown = list.names.slice(0, NAMES_SHOWN);
            const rest = list.count - shown.length;
            return (
              <div key={label}>
                <dt>{t(label)}</dt>
                <dd>
                  {shown.join(', ')}
                  {rest > 0 && ` ${t('news.modpack.andMore', { count: rest })}`}
                </dd>
              </div>
            );
          })}
        </dl>
      )}
    </>
  );
}
