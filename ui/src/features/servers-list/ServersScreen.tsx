import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { isLauncherError } from '../../ipc/errors';
import type { ServerFacets } from '../../ipc/schema';
import { useServersList, useUnfollow } from '../../state/queries';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { Sentinel } from '../../ui/Sentinel';
import { PublicServerCard } from './PublicServerCard';
import { ServerFilters } from './ServerFilters';
import { isFiltered, NO_FILTERS, rememberServerFilters, sessionServerFilters, type ListFilters } from './session';
import { PUBLIC_HEADING, YourServers } from './YourServers';
import { useFollowed } from './yours';

/** How many placeholder cards stand in while the first page loads. */
const SKELETONS = 4;

/** Perland's public servers: find one and press Play. The body is a stack of sections, so Your servers
 *  (#48) can take its slot above the list without moving it. */
export function ServersScreen() {
  const { t } = useTranslation();
  return (
    <ScreenFrame title={t('servers.title')} subtitle={t('servers.subtitle')} wide>
      <div className="grid gap-10">
        <YourServers />
        <PublicServers />
      </div>
    </ScreenFrame>
  );
}

function PublicServers() {
  const { t } = useTranslation();
  const [filters, setFilters] = useState<ListFilters>(sessionServerFilters);
  useEffect(() => rememberServerFilters(filters), [filters]);
  const update = (patch: Partial<ListFilters>) => setFilters((f) => ({ ...f, ...patch }));
  // A server the player follows keeps its public card here too (hiding it would make the count and filters lie), with Unfollow.
  const followed = useFollowed();
  const unfollow = useUnfollow();
  const menuFor = (invite: string | undefined) => (invite ? { onUnfollow: () => unfollow.mutate(invite) } : undefined);

  const list = useServersList({ ...filters, offset: 0 });
  const pages = list.data?.pages ?? [];
  const servers = pages.flatMap((page) => page.servers);
  // The facets describe the whole list, not the filtered page, so the last ones seen stay while a new filter loads.
  const [facets, setFacets] = useState<ServerFacets | null>(null);
  const latest = pages[0]?.facets ?? null;
  if (latest && latest !== facets) setFacets(latest);

  const { fetchNextPage, hasNextPage, isFetchingNextPage } = list;
  const loadMore = useCallback(() => {
    if (hasNextPage && !isFetchingNextPage) void fetchNextPage();
  }, [fetchNextPage, hasNextPage, isFetchingNextPage]);

  // Off at the Engine, or an Engine without the list: a calm word, never an error.
  const closed = list.isError && isLauncherError(list.error) && list.error.code === 'server_list_closed';

  let body: ReactNode;
  if (list.isPending) {
    body = (
      <>
        <p role="status" className="sr-only">
          {t('common.loading')}
        </p>
        <div className="servers-grid" aria-hidden>
          {Array.from({ length: SKELETONS }, (_, i) => (
            <div key={i} className="server-tile" data-skeleton="" />
          ))}
        </div>
      </>
    );
  } else if (closed) {
    body = <p className="empty-slot">{t('errors.server_list_closed.message')}</p>;
  } else if (list.isError && servers.length === 0) {
    body = <ErrorCard error={list.error} actions={<Button onClick={() => void list.refetch()}>{t('common.retry')}</Button>} />;
  } else if (servers.length === 0) {
    body = isFiltered(filters) ? (
      <div className="empty-slot servers-nothing">
        <p>{t('servers.noMatch')}</p>
        <Button variant="ghost" onClick={() => setFilters((f) => ({ ...NO_FILTERS, sort: f.sort }))}>
          {t('servers.clearFilters')}
        </Button>
      </div>
    ) : (
      <p className="empty-slot">{t('servers.empty')}</p>
    );
  } else {
    body = (
      <>
        <ul className="servers-grid">
          {servers.map((card) => (
            <li key={card.token} className="grid">
              <PublicServerCard card={card} menu={menuFor(followed.get(card.token))} />
            </li>
          ))}
        </ul>
        {hasNextPage && (
          <div className="grid justify-items-center gap-2">
            {list.isFetchNextPageError ? (
              <ErrorCard error={list.error} actions={<Button onClick={loadMore}>{t('common.retry')}</Button>} />
            ) : (
              <>
                <Sentinel onVisible={loadMore} />
                <Button aria-disabled={isFetchingNextPage || undefined} onClick={loadMore}>
                  {isFetchingNextPage ? t('common.loading') : t('servers.loadMore')}
                </Button>
              </>
            )}
          </div>
        )}
      </>
    );
  }

  return (
    <section className="grid gap-4" aria-labelledby="public-servers">
      <h2 id={PUBLIC_HEADING} tabIndex={-1} className="text-[1.125rem] leading-tight font-bold">
        {t('servers.public')}
      </h2>
      {!closed && <ServerFilters filters={filters} facets={facets} onChange={update} />}
      {body}
    </section>
  );
}
