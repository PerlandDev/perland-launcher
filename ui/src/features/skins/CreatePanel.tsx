// The Create tab's column: categories at left, the pieces of the open category with an On badge for worn ones, and the
// picked worn piece's colour areas with Take off and Make part of the skin. Looks applies a whole catalog look; Face
// opens with the ready-made expressions. A face piece the painting hides gets a hint with a way to start clean from
// pieces. Pointing at a piece, a look or an expression tries it on the figure without changing the skin (see tryOn).
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';
import type { CatalogLook, CatalogPiece, SkinLook } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { AreaColours } from './AreaColours';
import { CATEGORIES, HEAD_SLOTS } from './categories';
import type { Lang } from './catalogSearch';
import { CatalogThumb } from './CatalogThumb';
import { ExpressionRow } from './ExpressionRow';
import { focusFor, type Focus } from './framing';
import { applyLook, putOn } from './look';
import { tryHandlers, type Candidate, type TryOnHandle } from './tryOn';
import type { Catalog } from './useCatalog';
import type { LookEditor } from './useLook';
import { usePages } from './usePages';
import { useThumbs, type Thumbs } from './useThumbs';

function PieceTile({
  piece,
  lang,
  worn,
  picked,
  thumb,
  onPick,
  tryOn,
  trying,
}: {
  piece: CatalogPiece;
  lang: Lang;
  worn: boolean;
  picked: boolean;
  /** What draws the thumbnail (see `useThumbs`): the head alone for a piece worn on the head, else the whole figure. */
  thumb: CSSProperties | null;
  onPick: () => void;
  tryOn?: TryOnHandle;
  /** What pointing at the tile tries on. */
  trying: Candidate;
}) {
  const { t } = useTranslation();
  return (
    <li>
      <button type="button" className="piece-tile" title={piece.name[lang]} aria-current={picked || undefined} data-worn={worn || undefined} onClick={onPick} {...tryHandlers(tryOn, trying)}>
        {HEAD_SLOTS.has(piece.slot) ? (
          <span aria-hidden className="piece-thumb piece-thumb-head" style={thumb ?? undefined} />
        ) : (
          <span className="piece-thumb">
            <CatalogThumb style={thumb} />
          </span>
        )}
        <span className="piece-name">{piece.name[lang]}</span>
        {worn && <span className="piece-on">{t('skins.create.on')}</span>}
      </button>
    </li>
  );
}

/** The catalog's looks, a page at a time (see usePages): hundreds of tiles never mount at once. */
function LooksGrid({ looks, lang, thumbs, onPick, tryOn }: { looks: readonly CatalogLook[]; lang: Lang; thumbs: Thumbs; onPick: (look: CatalogLook) => void; tryOn?: TryOnHandle }) {
  const { t } = useTranslation();
  const { drawn, left, more, end } = usePages(lang, looks.length, '.create-side');
  return (
    <div className="create-group">
      <h3 className="create-heading">
        {t('skins.create.category.looks')} · <span className="tabular">{looks.length}</span>
      </h3>
      <ul className="piece-grid" aria-label={t('skins.create.category.looks')}>
        {looks.slice(0, drawn).map((look) => (
          <li key={look.id}>
            <button
              type="button"
              className="piece-tile"
              aria-label={look.name[lang]}
              title={look.name[lang]}
              onClick={() => onPick(look)}
              {...tryHandlers(tryOn, { key: `look:${look.id}`, name: look.name[lang], change: (l) => applyLook(l, look) })}
            >
              <span className="piece-thumb">
                <CatalogThumb style={thumbs.look(look)} />
              </span>
              <span className="piece-name">{look.name[lang]}</span>
            </button>
          </li>
        ))}
      </ul>
      {left > 0 && (
        <div ref={end} className="look-more">
          <Button onClick={more}>{t('skins.catalog.showMore')}</Button>
        </div>
      )}
    </div>
  );
}

/** `onBuildFromPieces` starts a new skin from pieces (absent when the catalog can't build one). */
export function CreatePanel({
  catalog,
  look,
  editor,
  lang,
  onBuildFromPieces,
  tryOn,
  onFocus,
}: {
  catalog: Catalog;
  look: SkinLook | null;
  editor: LookEditor;
  lang: Lang;
  onBuildFromPieces?: () => void;
  /** Tries what a tile would put on, on the figure, while the tile is pointed at. */
  tryOn?: TryOnHandle;
  /** Told what the stage camera should frame: the part the open category dresses (see framing), named by the category. */
  onFocus?: (focus: Focus, category: string) => void;
}) {
  const { t } = useTranslation();
  const [category, setCategory] = useState('tops');
  const [picked, setPicked] = useState<string | null>(null);
  const [baking, setBaking] = useState(false);
  const [applying, setApplying] = useState<CatalogLook | null>(null);
  // The shown piece's colours: under every piece of the category, however many there are.
  const colours = useRef<HTMLElement>(null);
  // Another category starts at its top: the panel would otherwise stay as far down as the last one was scrolled.
  const side = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    if (side.current) side.current.scrollTop = 0;
  }, [category]);
  const { manifest } = catalog;
  // Every tile's thumbnail is cut from the catalog's two sheets: a category of any size costs no file of its own.
  const thumbs = useThumbs(catalog);
  // The camera frames what the category dresses; where a category holds several parts (Accessories), the piece picked
  // there says which. With no catalog there is nothing to dress: the whole figure.
  const slots = CATEGORIES.find((c) => c.id === category)?.slots ?? [];
  const pickedSlot = manifest?.pieces.find((p) => p.id === picked && slots.includes(p.slot))?.slot;
  const focus = manifest ? focusFor(category, pickedSlot) : 'figure';
  useEffect(() => onFocus?.(focus, category), [onFocus, focus, category]);

  if (!manifest) {
    return (
      <aside className="skins-side create-side" aria-label={t('skins.tab.create')}>
        {catalog.loading ? <p className="text-sm text-ink-dim">{t('skins.create.loading')}</p> : <p className="empty-slot">{t('skins.catalog.unavailable')}</p>}
        {catalog.status === 'too_new' && <p className="info-line">{t('skins.catalog.tooNew')}</p>}
      </aside>
    );
  }

  const worn = look?.pieces ?? [];
  const wornIds = new Set(worn.map((w) => w.id));
  const open = CATEGORIES.find((c) => c.id === category) ?? CATEGORIES[0]!;
  const inSlots = (slots: readonly string[]) => manifest.pieces.filter((p) => slots.includes(p.slot) && (!p.retired || wornIds.has(p.id)));
  // The piece whose colours show: the one picked here while it's worn, else the first worn piece of the category.
  const firstWorn = open.slots.flatMap((slot) => inSlots([slot])).find((p) => wornIds.has(p.id));
  const shownId = picked && wornIds.has(picked) && inSlots(open.slots).some((p) => p.id === picked) ? picked : (firstWorn?.id ?? null);
  const shown = shownId ? manifest.pieces.find((p) => p.id === shownId) : undefined;
  const shownWorn = shownId ? worn.find((w) => w.id === shownId) : undefined;

  const pick = (piece: CatalogPiece) => {
    tryOn?.commit();
    setPicked(piece.id);
    if (!wornIds.has(piece.id)) editor.putOn(piece);
  };
  const pickLook = (l: CatalogLook) => {
    tryOn?.commit();
    if (worn.length > 0) setApplying(l);
    else editor.applyLook(l);
  };
  // The Colours key: the colours come into view and the keyboard lands on the first of them.
  const toColours = () => {
    const section = colours.current;
    section?.scrollIntoView?.({ block: 'start' });
    section?.querySelector<HTMLElement>('input, button')?.focus({ preventScroll: true });
  };
  const openCategory = (id: string) => {
    // The tiles under the pointer go away without a pointer leave.
    tryOn?.clear();
    setCategory(id);
  };
  const groups = open.slots.length === 1 ? [{ slot: open.slots[0]!, label: t(`skins.create.category.${open.id}`) }] : open.slots.map((slot) => ({ slot, label: t(`skins.create.slot.${slot}`) }));

  return (
    <aside ref={side} className="skins-side create-side" aria-label={t('skins.tab.create')}>
      <div className="create">
        <div role="group" aria-label={t('skins.create.categories')} className="create-categories">
          {CATEGORIES.map((c) => (
            <button key={c.id} type="button" className="create-category" aria-pressed={c.id === category} onClick={() => openCategory(c.id)}>
              {t(`skins.create.category.${c.id}`)}
            </button>
          ))}
        </div>
        <div className="create-pieces">
          {open.id === 'face' && <ExpressionRow expressions={manifest.expressions} pieces={manifest.pieces} look={look} editor={editor} lang={lang} tryOn={tryOn} />}
          {/* Up here, under the expressions, so it shows where the player just clicked however long the lists get. */}
          {editor.paintingCovers && (
            <div className="create-covered" role="status">
              <p className="info-line">{t('skins.create.paintingCovers')}</p>
              {onBuildFromPieces && <Button onClick={onBuildFromPieces}>{t('skins.create.paintingCoversAction')}</Button>}
            </div>
          )}
          {open.id === 'looks' ? (
            <LooksGrid looks={manifest.looks.filter((l) => !l.retired)} lang={lang} thumbs={thumbs} onPick={pickLook} tryOn={tryOn} />
          ) : (
            groups.map(({ slot, label }) => {
              const pieces = inSlots([slot]);
              if (pieces.length === 0 && groups.length > 1) return null;
              return (
                <div key={slot} className="create-group">
                  <h3 className="create-heading">
                    {label} · <span className="tabular">{t('skins.create.count', { count: pieces.length })}</span>
                  </h3>
                  {pieces.length === 0 ? (
                    <p className="text-sm text-ink-dim">{t('skins.create.empty')}</p>
                  ) : (
                    <ul className="piece-grid" data-head={HEAD_SLOTS.has(slot) || undefined} aria-label={label}>
                      {pieces.map((p) => (
                        <PieceTile
                          key={p.id}
                          piece={p}
                          lang={lang}
                          worn={wornIds.has(p.id)}
                          picked={p.id === shownId}
                          thumb={HEAD_SLOTS.has(p.slot) ? thumbs.head(p) : thumbs.piece(p)}
                          onPick={() => pick(p)}
                          tryOn={tryOn}
                          trying={{ key: `piece:${p.id}`, name: p.name[lang], change: (l) => putOn(l, p, manifest.pieces) }}
                        />
                      ))}
                    </ul>
                  )}
                </div>
              );
            })
          )}
          {open.id !== 'looks' && groups.every(({ slot }) => inSlots([slot]).length === 0) && groups.length > 1 && <p className="text-sm text-ink-dim">{t('skins.create.empty')}</p>}

          {open.id !== 'looks' &&
            (shown && shownWorn ? (
              <AreaColours ref={colours} piece={shown} worn={shownWorn} lang={lang} onColour={(area, hex, live) => editor.setColour(shown.id, area, hex, live)} />
            ) : (
              <p className="text-sm text-ink-dim">{t('skins.create.pick')}</p>
            ))}

          {(shownWorn || worn.length > 0) && (
            <div className="create-actions">
              {/* The row stays in view while the pieces scroll: from it the colours are one press away. */}
              {shown && shownWorn && shown.areas.length > 0 && open.id !== 'looks' && (
                <Button title={t('skins.create.colours', { name: shown.name[lang] })} onClick={toColours}>
                  {t('skins.create.toColours')}
                </Button>
              )}
              {shownWorn && open.id !== 'looks' && <Button onClick={() => editor.takeOff(shownWorn.id)}>{t('skins.create.takeOff')}</Button>}
              {worn.length > 0 && <Button onClick={() => setBaking(true)}>{t('skins.create.bake')}</Button>}
            </div>
          )}
        </div>
      </div>

      <Dialog
        open={baking}
        onClose={() => setBaking(false)}
        title={t('skins.create.bakeTitle')}
        actions={
          <>
            <Button variant="ghost" onClick={() => setBaking(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                setBaking(false);
                editor.bakeAll();
              }}
            >
              {t('skins.create.bake')}
            </Button>
          </>
        }
      >
        <p>{t('skins.create.bakeBody')}</p>
      </Dialog>

      <Dialog
        open={applying !== null}
        onClose={() => setApplying(null)}
        title={t('skins.create.lookTitle', { name: applying?.name[lang] ?? '' })}
        actions={
          <>
            <Button variant="ghost" onClick={() => setApplying(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                const l = applying;
                setApplying(null);
                if (l) editor.applyLook(l);
              }}
            >
              {t('skins.create.lookConfirm')}
            </Button>
          </>
        }
      >
        <p>{t('skins.create.lookBody')}</p>
      </Dialog>
    </aside>
  );
}
