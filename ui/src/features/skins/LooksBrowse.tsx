// The Catalog tab's shelves and grid: search, tag keys with their counts, arms and order, then the looks. With nothing
// searched or filtered there is a shelf per tag over the grid of everything; any search or filter leaves just the
// grid. The grid draws its looks a page at a time as the player scrolls, so a catalog of hundreds stays light.
import { memo, useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronLeft, ChevronRight, Shuffle } from 'pixelarticons/react';
import type { CatalogLook, CatalogManifest, CatalogStatus } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { GRID_PAGE, lookShelves, NO_FILTER, noFilter, tagFacets, type Lang, type LookFilter, type LookShelf } from './catalogSearch';
import { CatalogThumb } from './CatalogThumb';
import type { Thumbs } from './useThumbs';

const LookTile = memo(function LookTile({ look, name, picked, thumb, onOpen }: { look: CatalogLook; name: string; picked: boolean; thumb: CSSProperties | null; onOpen: (look: CatalogLook) => void }) {
  return (
    <li>
      <button type="button" className="look-tile" data-look={look.id} aria-pressed={picked} aria-label={name} title={name} onClick={() => onOpen(look)}>
        <CatalogThumb style={thumb} className="look-tile-thumb" />
        <span className="look-tile-name">{name}</span>
      </button>
    </li>
  );
});

/** A select set into a small key, as on the Servers screen: its name in dim ink, then the choice. Set to anything but
 *  its first option, a filter sinks into its slot with the lime lamp; an order always has a value, so it never does. */
function FilterKey({ label, value, options, filter = true, onChange }: { label: string; value: string; options: { value: string; label: string }[]; filter?: boolean; onChange: (value: string) => void }) {
  const id = useId();
  return (
    <div className="filter-key" data-set={filter && value !== '' ? '' : undefined}>
      <label htmlFor={id}>{label}</label>
      <select id={id} value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  );
}

/** One tag's shelf: its name and count, See all (which chooses the tag), and its first looks in a row that scrolls
 *  sideways by wheel, by drag or by the two arrow keys. */
function Shelf({ shelf, lang, picked, thumbs, onOpen, onSeeAll }: { shelf: LookShelf; lang: Lang; picked: string | null; thumbs: Thumbs; onOpen: (look: CatalogLook) => void; onSeeAll: (tag: string) => void }) {
  const { t } = useTranslation();
  const id = useId();
  const strip = useRef<HTMLUListElement>(null);
  const turn = (by: 1 | -1) => {
    const el = strip.current;
    if (!el) return;
    const calm = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
    el.scrollBy?.({ left: by * el.clientWidth * 0.8, behavior: calm ? 'auto' : 'smooth' });
  };
  return (
    <section className="look-shelf" aria-labelledby={id}>
      <header className="look-shelf-head">
        <h3 id={id} className="look-shelf-name">
          {shelf.name} <span className="look-count tabular">{shelf.count}</span>
        </h3>
        <Button variant="ghost" className="look-shelf-all" aria-label={t('skins.catalog.seeAllTag', { tag: shelf.name })} onClick={() => onSeeAll(shelf.id)}>
          {t('skins.catalog.seeAll')}
        </Button>
        {shelf.looks.length > 3 && (
          <>
            <Button variant="ghost" size="icon" icon={<ChevronLeft />} aria-label={t('skins.catalog.shelfBack', { tag: shelf.name })} title={t('skins.catalog.shelfBack', { tag: shelf.name })} onClick={() => turn(-1)} />
            <Button variant="ghost" size="icon" icon={<ChevronRight />} aria-label={t('skins.catalog.shelfForward', { tag: shelf.name })} title={t('skins.catalog.shelfForward', { tag: shelf.name })} onClick={() => turn(1)} />
          </>
        )}
      </header>
      <ul ref={strip} className="look-strip" aria-labelledby={id}>
        {shelf.looks.map((look) => (
          <LookTile key={look.id} look={look} name={look.name[lang]} picked={look.id === picked} thumb={thumbs.look(look)} onOpen={onOpen} />
        ))}
      </ul>
    </section>
  );
}

/** The grid of `looks` (`showing` names what they are: the filter and the language), a page at a time: more are drawn when its end comes near the bottom of what's scrolled into
 *  view, and Show more does the same by hand (and where the webview has no IntersectionObserver). */
function Grid({ looks, showing, lang, picked, thumbs, onOpen }: { looks: CatalogLook[]; showing: string; lang: Lang; picked: string | null; thumbs: Thumbs; onOpen: (look: CatalogLook) => void }) {
  const { t } = useTranslation();
  // How many are drawn, for what's showing: something else showing starts from the first page again.
  const [page, setPage] = useState({ showing, drawn: GRID_PAGE });
  const drawn = page.showing === showing ? page.drawn : GRID_PAGE;
  const more = useCallback(() => setPage((p) => ({ showing, drawn: (p.showing === showing ? p.drawn : GRID_PAGE) + GRID_PAGE })), [showing]);
  const end = useRef<HTMLDivElement>(null);
  const left = looks.length - drawn;
  useEffect(() => {
    const el = end.current;
    if (!el || left <= 0 || typeof IntersectionObserver === 'undefined') return;
    // Watched again after every page: an end still in view after one page asks for the next.
    const observer = new IntersectionObserver((entries) => entries.some((entry) => entry.isIntersecting) && more(), { root: el.closest('.looks-scroll'), rootMargin: '0px 0px 480px 0px' });
    observer.observe(el);
    return () => observer.disconnect();
  }, [left, more]);
  return (
    <>
      <ul className="look-grid" aria-label={t('skins.catalog.looks')}>
        {looks.slice(0, drawn).map((look) => (
          <LookTile key={look.id} look={look} name={look.name[lang]} picked={look.id === picked} thumb={thumbs.look(look)} onOpen={onOpen} />
        ))}
      </ul>
      {left > 0 && (
        <div ref={end} className="look-more">
          <Button onClick={more}>{t('skins.catalog.showMore')}</Button>
        </div>
      )}
    </>
  );
}

export interface LooksBrowseProps {
  manifest: CatalogManifest;
  status: CatalogStatus;
  lang: Lang;
  thumbs: Thumbs;
  filter: LookFilter;
  /** What `filter` shows, in its order. */
  looks: CatalogLook[];
  /** The look shown beside the grid: its tiles sit pressed. */
  picked: string | null;
  onFilter: (filter: LookFilter) => void;
  onOpen: (look: CatalogLook) => void;
  /** Opens a look of `looks` picked at random. */
  onRandom: () => void;
}

export function LooksBrowse({ manifest, status, lang, thumbs, filter, looks, picked, onFilter, onOpen, onRandom }: LooksBrowseProps) {
  const { t } = useTranslation();
  const facets = useMemo(() => tagFacets(manifest, filter, lang), [manifest, filter, lang]);
  const everything = noFilter(filter);
  const shelves = useMemo(() => (everything ? lookShelves(manifest, looks, lang) : []), [everything, manifest, looks, lang]);
  const scroll = useRef<HTMLDivElement>(null);
  // What's shown changed: it starts from its top, and the grid from its first page.
  const showing = `${lang}\n${filter.query}\n${filter.tag}\n${filter.arms}\n${filter.sort}`;
  useLayoutEffect(() => {
    if (scroll.current) scroll.current.scrollTop = 0;
  }, [showing]);
  const seeAll = useCallback((tag: string) => onFilter({ ...filter, tag }), [filter, onFilter]);

  const chip = (id: string, name: ReactNode, count: number) => (
    <button key={id} type="button" className="chip" aria-pressed={filter.tag === id} onClick={() => onFilter({ ...filter, tag: id })}>
      {name} <span className="look-count tabular">{count}</span>
    </button>
  );

  return (
    <div className="looks-browse">
      <div ref={scroll} className="looks-scroll">
        <div className="looks-bar">
          <input type="search" className="field-input" aria-label={t('skins.catalog.search')} placeholder={t('skins.catalog.searchPlaceholder')} value={filter.query} onChange={(e) => onFilter({ ...filter, query: e.target.value })} />
          <Button icon={<Shuffle />} disabled={looks.length === 0} onClick={onRandom}>
            {t('skins.catalog.random')}
          </Button>
        </div>
        <div role="group" aria-label={t('skins.catalog.tags')} className="browse-chips">
          {chip('', t('skins.catalog.all'), facets.total)}
          {facets.tags.map((tag) => chip(tag.id, tag.name, tag.count))}
        </div>
        <div className="looks-sub">
          <p className="looks-total tabular" role="status">
            {t('skins.catalog.count', { count: looks.length })}
          </p>
          <FilterKey
            label={t('skins.arms')}
            value={filter.arms}
            options={[
              { value: '', label: t('skins.catalog.all') },
              { value: 'classic', label: t('skins.catalog.armsClassic') },
              { value: 'slim', label: t('skins.catalog.armsSlim') },
            ]}
            onChange={(arms) => onFilter({ ...filter, arms: arms as LookFilter['arms'] })}
          />
          <FilterKey
            label={t('skins.catalog.sort')}
            value={filter.sort}
            filter={false}
            options={[
              { value: 'featured', label: t('skins.catalog.sortFeatured') },
              { value: 'name', label: t('skins.catalog.sortName') },
            ]}
            onChange={(sort) => onFilter({ ...filter, sort: sort as LookFilter['sort'] })}
          />
        </div>
        {status === 'cached' && <p className="text-sm text-ink-dim">{t('skins.catalog.cached')}</p>}
        {status === 'too_new' && <p className="info-line">{t('skins.catalog.tooNew')}</p>}

        {shelves.length > 0 && (
          <div className="look-shelves" role="group" aria-label={t('skins.catalog.shelves')}>
            {shelves.map((shelf) => (
              <Shelf key={shelf.id} shelf={shelf} lang={lang} picked={picked} thumbs={thumbs} onOpen={onOpen} onSeeAll={seeAll} />
            ))}
          </div>
        )}
        {shelves.length > 0 && (
          <h3 className="look-shelf-name looks-all">
            {t('skins.catalog.allLooks')} <span className="look-count tabular">{looks.length}</span>
          </h3>
        )}
        <Grid showing={showing} looks={looks} lang={lang} picked={picked} thumbs={thumbs} onOpen={onOpen} />
        {looks.length === 0 && (
          <div className="looks-none">
            <p className="empty-slot text-ink-dim">{t('skins.catalog.none')}</p>
            <Button onClick={() => onFilter({ ...NO_FILTER, sort: filter.sort })}>{t('skins.catalog.clear')}</Button>
          </div>
        )}
      </div>
    </div>
  );
}
