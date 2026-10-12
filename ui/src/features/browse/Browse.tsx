import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { describeError, isLauncherError } from '../../ipc/errors';
import type { ContentKind, LoaderKind, SortIndex } from '../../ipc/schema';
import { useBrowseCategories, useBrowseInstalled, useBrowseSearch, useInstances, useMcVersions } from '../../state/queries';
import { navigate, type Route } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { formatCount } from '../../ui/format';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { Select } from '../../ui/Select';
import { useDebounced } from '../../ui/useDebounced';
import { loaderName } from '../home/labels';
import { categoryLabel, gameLabel } from './labels';
import { ProjectPage } from './ProjectPage';
import { ResultRow } from './ResultRow';
import { RowInstall } from './RowInstall';
import { remember, sessionFilters, type Filters } from './session';

export type BrowseRoute = Extract<Route, { name: 'browse' }>;

const ALL_KINDS: ContentKind[] = ['modpack', 'mod', 'resource_pack', 'shader'];
const SORTS: SortIndex[] = ['relevance', 'downloads', 'follows', 'newest', 'updated'];
const LOADERS: LoaderKind[] = ['fabric', 'quilt', 'forge', 'neoforge'];
const VISIBLE_CATEGORIES = 8;

export function Browse({ route }: { route: BrowseRoute }) {
  if (route.project) return <ProjectPage key={route.project} route={route} project={route.project} />;
  // A new kind or scope starts from that kind's remembered filters.
  return <BrowseList key={`${route.kind}:${route.instance ?? ''}`} route={route} />;
}

function Sentinel({ onVisible }: { onVisible: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof IntersectionObserver === 'undefined') return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) onVisible();
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [onVisible]);
  return <div ref={ref} aria-hidden className="h-px" />;
}

/** The wait the core asked for; anything unusable falls back to 10 s so the countdown still ends. */
function retrySeconds(value: string | undefined): number {
  const seconds = Number(value ?? '10');
  return Number.isFinite(seconds) && seconds >= 0 ? seconds : 10;
}

/** Why the next page didn't load. The first page stays; Load more is the retry. */
function PageError({ error }: { error: unknown }) {
  const { t, i18n } = useTranslation();
  let text: string;
  if (isLauncherError(error) && error.code === 'host_unreachable') text = t('browse.offline');
  else if (isLauncherError(error) && error.code === 'browse_rate_limited') {
    text = t('browse.rateLimited', { seconds: retrySeconds(error.params.seconds) });
  } else text = describeError(i18n, error).message;
  return (
    <p role="alert" className="text-center">
      {text}
    </p>
  );
}

function RateLimited({ seconds, onRetry }: { seconds: number; onRetry: () => unknown }) {
  const { t } = useTranslation();
  const [left, setLeft] = useState(seconds);
  useEffect(() => {
    if (left <= 0) {
      void onRetry();
      return;
    }
    const timer = setTimeout(() => setLeft((n) => n - 1), 1000);
    return () => clearTimeout(timer);
  }, [left, onRetry]);
  return (
    <p role="status" className="panel p-4">
      {t('browse.rateLimited', { seconds: Math.max(left, 0) })}
    </p>
  );
}

function BrowseList({ route }: { route: BrowseRoute }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const scoped = route.instance !== undefined;
  const instances = useInstances();
  const scope = scoped ? (instances.data?.instances.find((i) => i.id === route.instance) ?? null) : null;
  const [filters, setFilters] = useState<Filters>(() => sessionFilters(route.kind, scoped));
  useEffect(() => remember(route.kind, scoped, filters), [route.kind, scoped, filters]);
  const update = (patch: Partial<Filters>) => setFilters((f) => ({ ...f, ...patch }));

  const query = useDebounced(filters.query.trim(), 300);
  const sort = filters.sort ?? (query === '' ? 'downloads' : 'relevance');
  const usesLoader = route.kind === 'mod' || route.kind === 'modpack';
  const results = useBrowseSearch({
    query,
    kind: route.kind,
    instance: route.instance ?? null,
    minecraft: scoped ? null : filters.minecraft,
    loader: scoped || !usesLoader ? null : filters.loader,
    categories: filters.categories,
    sort,
  });
  const installed = useBrowseInstalled(route.instance ?? null);
  const categories = useBrowseCategories(route.kind);
  const mcVersions = useMcVersions(!scoped);
  const [allChips, setAllChips] = useState(false);

  const searchRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (event.key !== '/' || event.ctrlKey || event.metaKey || event.altKey || target?.closest('input, textarea, select, [contenteditable="true"]')) return;
      event.preventDefault();
      searchRef.current?.focus();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const { fetchNextPage } = results;
  const { hasNextPage, isFetchingNextPage } = results;
  const loadMore = useCallback(() => {
    if (hasNextPage && !isFetchingNextPage) void fetchNextPage();
  }, [fetchNextPage, hasNextPage, isFetchingNextPage]);

  // Scoped: never modpacks, even before the instance has loaded; a vanilla instance takes only resource packs.
  const kinds = scoped
    ? ALL_KINDS.filter((k) => k !== 'modpack' && (!scope || scope.game.loader !== null || k === 'resource_pack'))
    : ALL_KINDS;
  const hits = results.data?.pages.flatMap((page) => page.hits) ?? [];
  const total = results.data?.pages[0]?.total ?? 0;
  const chips = categories.data ?? [];
  const shownChips = allChips ? chips : chips.slice(0, VISIBLE_CATEGORIES);
  const toggle = (slug: string) =>
    update({
      categories: filters.categories.includes(slug) ? filters.categories.filter((c) => c !== slug) : [...filters.categories, slug],
    });
  const clear = () => update({ query: '', categories: [], minecraft: null, loader: null });

  let body: ReactNode;
  if (results.isPending) {
    body = (
      <p role="status" className="text-ink-dim">
        {t('common.loading')}
      </p>
    );
  } else if (results.isError && hits.length === 0) {
    const error = results.error;
    if (isLauncherError(error) && error.code === 'host_unreachable') {
      body = (
        <div className="panel flex items-center justify-between gap-4 p-4">
          <p>{t('browse.offline')}</p>
          <Button onClick={() => void results.refetch()}>{t('common.retry')}</Button>
        </div>
      );
    } else if (isLauncherError(error) && error.code === 'browse_rate_limited') {
      body = <RateLimited key={results.errorUpdatedAt} seconds={retrySeconds(error.params.seconds)} onRetry={results.refetch} />;
    } else {
      body = <ErrorCard error={error} actions={<Button onClick={() => void results.refetch()}>{t('common.retry')}</Button>} />;
    }
  } else if (hits.length === 0) {
    body = (
      <div className="grid justify-items-start gap-3">
        <p>{query ? t('browse.nothing', { query }) : t('browse.empty')}</p>
        {(query || filters.categories.length > 0 || filters.minecraft || filters.loader) && (
          <Button onClick={clear}>{t('browse.clearFilters')}</Button>
        )}
      </div>
    );
  } else {
    body = (
      <>
        <p className="tabular text-sm text-ink-dim">{t('browse.results', { count: total, shown: formatCount(total, lang) })}</p>
        <ul className="browse-list">
          {hits.map((hit) => (
            <ResultRow
              key={hit.projectId}
              hit={hit}
              scoped={scoped}
              installed={installed.data?.includes(hit.projectId) ?? false}
              onOpen={() => navigate({ ...route, project: hit.slug })}
              action={scope ? <RowInstall hit={hit} instanceId={scope.id} /> : undefined}
            />
          ))}
        </ul>
        {results.hasNextPage ? (
          <div className="grid justify-items-center gap-2">
            {results.isError && <PageError error={results.error} />}
            {!results.isError && <Sentinel onVisible={loadMore} />}
            <Button disabled={results.isFetchingNextPage} onClick={loadMore}>
              {results.isFetchingNextPage ? t('common.loading') : t('browse.loadMore')}
            </Button>
          </div>
        ) : (
          <p className="text-center text-ink-dim">{t('browse.end')}</p>
        )}
      </>
    );
  }

  return (
    <ScreenFrame title={t('browse.title')} wide onBack={scope ? () => navigate({ name: 'mods', id: scope.id }) : undefined}>
      <div className="grid gap-5">
        {scope && (
          <div className="browse-scope">
            <p>{t('browse.scope', { name: scope.name, game: gameLabel(scope.game.minecraft, scope.game.loader?.kind ?? null) })}</p>
            <Button variant="ghost" onClick={() => navigate({ name: 'browse', kind: route.kind })}>
              {t('browse.dropScope')}
            </Button>
          </div>
        )}
        <div role="tablist" aria-label={t('browse.title')} className="flex flex-wrap gap-2">
          {kinds.map((kind) => (
            <button
              key={kind}
              type="button"
              role="tab"
              className="seg"
              aria-selected={kind === route.kind}
              data-checked={kind === route.kind ? '' : undefined}
              onClick={() => navigate({ name: 'browse', kind, instance: route.instance })}
            >
              {t(`browse.kinds.${kind}`)}
            </button>
          ))}
        </div>
        <div className="browse-controls">
          <input
            ref={searchRef}
            type="search"
            className="field-input"
            aria-label={t('browse.search')}
            placeholder={t(`browse.placeholders.${route.kind}`)}
            value={filters.query}
            onChange={(e) => update({ query: e.target.value })}
          />
          <Select
            label={t('browse.sort')}
            value={sort}
            onChange={(value) => update({ sort: value as SortIndex })}
            options={SORTS.map((s) => ({ value: s, label: t(`browse.sorts.${s}`) }))}
          />
          {!scoped && (
            <Select
              label={t('browse.minecraft')}
              value={filters.minecraft ?? ''}
              onChange={(value) => update({ minecraft: value || null })}
              options={[
                { value: '', label: t('browse.anyVersion') },
                ...(mcVersions.data ?? []).filter((v) => v.kind === 'release').map((v) => ({ value: v.id, label: v.id })),
              ]}
            />
          )}
          {!scoped && usesLoader && (
            <Select
              label={t('browse.loader')}
              value={filters.loader ?? ''}
              onChange={(value) => update({ loader: (value || null) as LoaderKind | null })}
              options={[{ value: '', label: t('browse.anyLoader') }, ...LOADERS.map((l) => ({ value: l, label: loaderName(l) }))]}
            />
          )}
        </div>
        {chips.length > 0 && (
          <fieldset className="flex flex-wrap gap-2">
            <legend className="sr-only">{t('browse.categories')}</legend>
            {shownChips.map((slug) => (
              <button key={slug} type="button" className="chip" aria-pressed={filters.categories.includes(slug)} onClick={() => toggle(slug)}>
                {categoryLabel(i18n, slug)}
              </button>
            ))}
            {chips.length > VISIBLE_CATEGORIES && (
              <button type="button" className="chip-more" onClick={() => setAllChips((v) => !v)}>
                {allChips ? t('browse.fewerCategories') : t('browse.moreCategories')}
              </button>
            )}
          </fieldset>
        )}
        {body}
      </div>
    </ScreenFrame>
  );
}
