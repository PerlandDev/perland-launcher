// The Catalog tab: Perland's ready-made looks. Shelves and a grid to browse them (LooksBrowse); picking one opens its
// page (LookPage), with the look on a figure, what it's made of, and the keys that put it in the wardrobe. The looks
// stay as they were underneath, so going back lands where the player left.
import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { skipToken, useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { CatalogLook, SkinEntry } from '../../ipc/schema';
import { browseLooks, NO_FILTER, type Lang, type LookFilter } from './catalogSearch';
import './catalog.css';
import { LookPage } from './LookPage';
import { LooksBrowse } from './LooksBrowse';
import { useCatalog } from './useCatalog';
import { useThumbs } from './useThumbs';

export interface CatalogPanelProps {
  /** The account whose wardrobe is shown (null: the loose skins); an added look goes into it. */
  owner: string | null;
  /** A look was added to the wardrobe as this skin: the player stays in the catalog. */
  onAdded: (entry: SkinEntry) => void;
  /** Open this skin (a look just added, or added earlier) in Create. */
  onCustomize: (entry: SkinEntry) => void;
}

export function CatalogPanel({ owner, onAdded, onCustomize }: CatalogPanelProps) {
  const { t, i18n } = useTranslation();
  const lang: Lang = i18n.language.startsWith('es') ? 'es' : 'en';
  const catalog = useCatalog();
  const thumbs = useThumbs(catalog);
  const { manifest, status } = catalog;
  const [filter, setFilter] = useState<LookFilter>(NO_FILTER);
  // The look whose page is open.
  const [opened, setOpened] = useState<string | null>(null);
  // What opened the page (the tile or key pressed, and the look it opened), and where the keyboard lands when the page
  // closes: on a look's tile, or on the tag key the page chose.
  const origin = useRef<{ look: string; from: HTMLElement | null } | null>(null);
  const landing = useRef<{ look: string } | 'tag' | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const looks = useMemo(() => (manifest ? browseLooks(manifest, filter, lang) : []), [manifest, filter, lang]);
  const look = useMemo(() => manifest?.looks.find((l) => l.id === opened && !l.retired) ?? null, [manifest, opened]);
  const at = look ? looks.indexOf(look) : -1;

  // The looks added from here, by look. The wardrobe (the Skins screen's own list, only read here) says whether each is
  // still there; one counts until the list has shown it once, since it joins the list a moment after it's saved.
  const [added, setAdded] = useState<ReadonlyMap<string, SkinEntry>>(new Map());
  const [listed, setListed] = useState<ReadonlySet<string>>(new Set());
  const wardrobe = useQuery<SkinEntry[]>({ queryKey: ['skins', owner], queryFn: skipToken }).data;
  const inWardrobe = useMemo(() => new Set(wardrobe?.map((e) => e.id)), [wardrobe]);
  const fresh = [...added.values()].filter((e) => inWardrobe.has(e.id) && !listed.has(e.id));
  if (fresh.length > 0) setListed(new Set([...listed, ...fresh.map((e) => e.id)]));
  const kept = look ? (added.get(look.id) ?? null) : null;
  const mine = kept && (wardrobe === undefined || inWardrobe.has(kept.id) || !listed.has(kept.id)) ? kept : null;

  const open = useCallback((picked: CatalogLook, from?: HTMLElement) => {
    const focused = document.activeElement;
    origin.current = { look: picked.id, from: from ?? (focused instanceof HTMLElement && focused !== document.body ? focused : null) };
    setOpened(picked.id);
  }, []);
  const back = useCallback(() => {
    landing.current = opened === null ? null : { look: opened };
    setOpened(null);
  }, [opened]);
  const step = useCallback(
    (by: 1 | -1) => {
      const next = looks[at + by];
      if (at >= 0 && next) setOpened(next.id);
    },
    [looks, at],
  );
  const random = useCallback(() => {
    // Another look than the one open, when there is one.
    const others = looks.length > 1 ? looks.filter((l) => l.id !== opened) : looks;
    const picked = others[Math.floor(Math.random() * others.length)];
    if (!picked) return;
    // From the looks it opens a page; from a page it is one more step.
    if (look) setOpened(picked.id);
    else open(picked);
  }, [looks, opened, look, open]);
  const showTag = useCallback((tag: string) => {
    setFilter((f) => ({ ...NO_FILTER, sort: f.sort, tag }));
    landing.current = 'tag';
    setOpened(null);
  }, []);
  const keep = useCallback(
    (from: CatalogLook, entry: SkinEntry, how: 'add' | 'customize') => {
      setAdded((m) => (m.get(from.id) === entry ? m : new Map(m).set(from.id, entry)));
      (how === 'add' ? onAdded : onCustomize)(entry);
    },
    [onAdded, onCustomize],
  );

  // Back from a look's page, the keyboard carries on from where the player is: the tile that opened the page if they
  // never left that look, else the grid's tile of the look last open (scrolled into view, if it's drawn), or the tag
  // key the page chose.
  const browsing = look === null;
  useLayoutEffect(() => {
    if (!browsing) return;
    const to = landing.current;
    landing.current = null;
    if (to === null) return;
    if (to === 'tag') return root.current?.querySelector<HTMLElement>('.look-tag-keys [aria-pressed="true"]')?.focus();
    const from = origin.current;
    const tiles = root.current?.querySelectorAll<HTMLElement>('.look-grid [data-look]') ?? [];
    (from?.look === to.look && from.from?.isConnected ? from.from : [...tiles].find((tile) => tile.dataset.look === to.look))?.focus();
  }, [browsing]);

  if (catalog.loading) {
    return (
      <div className="catalog-tab" aria-busy="true">
        <p className="catalog-message text-ink-dim">{t('skins.catalog.loading')}</p>
      </div>
    );
  }
  if (!manifest) {
    return (
      <div className="catalog-tab">
        <div className="catalog-message">
          <p className="empty-slot">{t('skins.catalog.unavailable')}</p>
          {status === 'too_new' && <p className="info-line">{t('skins.catalog.tooNew')}</p>}
        </div>
      </div>
    );
  }
  return (
    <div ref={root} className="catalog-tab">
      <LooksBrowse hidden={!browsing} manifest={manifest} status={status} lang={lang} thumbs={thumbs} filter={filter} looks={looks} onFilter={setFilter} onOpen={open} onRandom={random} />
      {look && (
        <LookPage
          catalog={catalog}
          manifest={manifest}
          look={look}
          lang={lang}
          owner={owner}
          at={at}
          total={looks.length}
          mine={mine}
          onStep={step}
          onBack={back}
          onRandom={random}
          onTag={showTag}
          onKept={keep}
        />
      )}
    </div>
  );
}
