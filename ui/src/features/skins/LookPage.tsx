// A catalog look's own page: the look on a figure the player can turn and pose, and beside it its name, its tags (each
// leads back to the looks of that tag), its arms and what it's made of, piece by piece with the colours it wears them
// in. Add to my wardrobe keeps it (the page's one lit key); Customize keeps it and opens it in Create. The arrows on
// the figure's well, and the left and right keys, step through the looks the player was browsing.
import { useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronLeft, ChevronRight, HumanArmsUp, MagicEdit, Shuffle } from 'pixelarticons/react';
import { commands } from '../../ipc/commands';
import type { CatalogLook, CatalogManifest, CatalogPiece, SkinEntry, SkinLook } from '../../ipc/schema';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Plus } from '../../ui/Icon';
import { Menu, type MenuHandle } from '../../ui/Menu';
import { useSingleFlight } from '../../ui/useSingleFlight';
import type { Lang } from './catalogSearch';
import { encodeTexture } from './codec';
import { compose } from './compose';
import { Pixels } from './paint';
import { POSES, type Pose } from './poses';
import { Stage, type StageHandle } from './Stage';
import type { Catalog } from './useCatalog';

const NO_PARTS_HIDDEN = new Set<string>();
const noop = () => {};

/** The body and the face every look starts from: listed apart from what it wears. */
const BASE_SLOTS: readonly string[] = ['body', 'eyes', 'brows', 'mouth'];
/** What a look wears, from head to toe; a slot this launcher doesn't know yet comes after, in the catalog's order. */
const WORN_SLOTS: readonly string[] = ['hat', 'hair', 'glasses', 'face_detail', 'beard', 'neck', 'top', 'back', 'hands', 'belt', 'bottom', 'shoes'];

/** A catalog look as a wardrobe look: its pieces with their colours. */
const asLook = (look: CatalogLook): SkinLook => ({ pieces: look.pieces.map((p) => ({ id: p.id, colours: { ...p.colours } })), painting: look.painting !== undefined });

/** The look put together on its own arm model: its painting (when it has one) under its pieces. */
async function composeLook(catalog: Catalog, look: CatalogLook) {
  const [layers, painting] = await Promise.all([catalog.layers(asLook(look), look.model), look.painting ? catalog.pixels(look.painting) : null]);
  return { texels: compose(painting, layers), painting };
}

interface Worn {
  piece: CatalogPiece;
  /** The colour of each of the piece's areas as the look wears it, in the piece's order. */
  colours: { id: string; name: string; value: string }[];
}

/** The look's pieces by slot, in `order`; pieces the catalog no longer names are left out. */
function wornBySlot(manifest: CatalogManifest, look: CatalogLook, lang: Lang, order: readonly string[]): { slot: string; worn: Worn[] }[] {
  const byId = new Map(manifest.pieces.map((p) => [p.id, p]));
  const groups = new Map<string, Worn[]>();
  for (const w of look.pieces) {
    const piece = byId.get(w.id);
    if (!piece) continue;
    const colours = piece.areas.map((a) => ({ id: a.id, name: a.name[lang], value: w.colours[a.id] ?? a.default }));
    groups.set(piece.slot, [...(groups.get(piece.slot) ?? []), { piece, colours }]);
  }
  return order.flatMap((slot) => (groups.has(slot) ? [{ slot, worn: groups.get(slot)! }] : []));
}

function WornList({ label, groups, lang }: { label: string; groups: { slot: string; worn: Worn[] }[]; lang: Lang }) {
  const { t } = useTranslation();
  const id = useId();
  if (groups.length === 0) return null;
  return (
    <div className="look-worn">
      <h3 id={id} className="look-worn-title">
        {label}
      </h3>
      <ul className="look-worn-list" aria-labelledby={id}>
        {groups.flatMap(({ slot, worn }) =>
          worn.map(({ piece, colours }, i) => (
            <li key={piece.id} className="look-worn-row">
              {/* A slot wearing two pieces names itself once. */}
              <span className="look-worn-slot">{i === 0 ? t(`skins.catalog.slot.${slot}`, { defaultValue: slot }) : ''}</span>
              <span className="look-worn-name">{piece.name[lang]}</span>
              {colours.length > 0 && (
                <span className="look-worn-colours" role="img" aria-label={t('skins.catalog.colours', { names: colours.map((c) => c.name).join(', ') })}>
                  {colours.map((c) => (
                    <span key={c.id} className="look-dot" title={c.name} style={{ backgroundColor: c.value }} />
                  ))}
                </span>
              )}
            </li>
          )),
        )}
      </ul>
    </div>
  );
}

const typing = (target: EventTarget | null) => target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement;

export interface LookPageProps {
  catalog: Catalog;
  manifest: CatalogManifest;
  look: CatalogLook;
  lang: Lang;
  /** The account whose wardrobe is shown (null: the loose skins); an added look goes into it. */
  owner: string | null;
  /** Where the look is among the looks being browsed, from 0 (-1: not among them), and how many they are. */
  at: number;
  total: number;
  /** The wardrobe skin this look was already added as, while the wardrobe still has it. */
  mine: SkinEntry | null;
  onStep: (by: 1 | -1) => void;
  onBack: () => void;
  onRandom: () => void;
  /** Back to the looks carrying this tag. */
  onTag: (tag: string) => void;
  /** The look is in the wardrobe as `entry` (just added, or `mine`), kept to stay in the catalog (`add`) or to open in
   *  Create (`customize`). */
  onKept: (look: CatalogLook, entry: SkinEntry, how: 'add' | 'customize') => void;
}

export function LookPage({ catalog, manifest, look, lang, owner, at, total, mine, onStep, onBack, onRandom, onTag, onKept }: LookPageProps) {
  const { t } = useTranslation();
  const nameId = useId();
  // The figure keeps the look it shows until the next one is put together, so stepping never blanks the well.
  const [figure, setFigure] = useState<{ look: CatalogLook; pixels: Pixels } | null>(null);
  const [pose, setPose] = useState<Pose>('still');
  const poseMenu = useRef<MenuHandle>(null);
  const stage = useRef<StageHandle>(null);
  const [busy, setBusy] = useState<'add' | 'customize' | null>(null);
  const once = useSingleFlight();

  useEffect(() => {
    let live = true;
    composeLook(catalog, look)
      .then(({ texels }) => live && setFigure({ look, pixels: new Pixels(texels) }))
      .catch(() => live && setFigure(null));
    return () => {
      live = false;
    };
  }, [catalog, look]);

  // The page takes the focus when it opens (the tile that opened it is out of sight now), and again when a step key
  // goes out under it at either end, so the keyboard is never left nowhere. Focus anywhere else is left alone.
  const page = useRef<HTMLElement>(null);
  useEffect(() => page.current?.focus({ preventScroll: true }), []);
  useEffect(() => {
    const focused = document.activeElement;
    if (!focused || focused === document.body || (page.current?.contains(focused) && focused.matches(':disabled'))) page.current?.focus({ preventScroll: true });
  }, [look.id]);

  const first = at <= 0;
  const last = at < 0 || at >= total - 1;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || typing(event.target)) return;
      // A menu or a dialog over the page keeps its own keys.
      if (document.querySelector('[role="menu"], dialog[open]')) return;
      if (event.key === 'ArrowLeft' && !first) onStep(-1);
      else if (event.key === 'ArrowRight' && !last) onStep(1);
      else if (event.key === 'Escape') onBack();
      else return;
      event.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [first, last, onStep, onBack]);

  const name = look.name[lang];
  const tagName = new Map(manifest.tags.map((tag) => [tag.id, tag.name[lang]]));
  const tags = look.tags.flatMap((id) => (tagName.has(id) ? [{ id, name: tagName.get(id)! }] : []));
  const wears = wornBySlot(manifest, look, lang, [...WORN_SLOTS, ...manifest.slots.filter((s) => !WORN_SLOTS.includes(s) && !BASE_SLOTS.includes(s))]);
  const base = wornBySlot(manifest, look, lang, BASE_SLOTS);

  /** Puts the look in the wardrobe (unless it's there already) and hands the skin on. */
  const keep = (how: 'add' | 'customize') =>
    once((done) => {
      setBusy(how);
      const stored = mine
        ? Promise.resolve(mine)
        : composeLook(catalog, look).then(async ({ texels, painting }) => {
            const texture = encodeTexture(texels);
            const entry = await commands.skinCreate(name.slice(0, 64), look.model, texture, owner);
            return commands.skinLookSave(entry.id, asLook(look), painting ? encodeTexture(painting) : null, texture);
          });
      void stored
        .then((entry) => {
          onKept(look, entry, how);
          // The figure hops: the look is the player's now.
          if (how === 'add') stage.current?.celebrate();
        })
        .catch(pushError)
        .finally(() => {
          setBusy(null);
          done();
        });
    });

  const poseLabel = `${t('skins.poseLabel')}: ${t(`skins.pose.${pose}`)}`;
  return (
    <section ref={page} tabIndex={-1} className="look-page" aria-labelledby={nameId}>
      <header className="look-page-bar">
        <Button variant="ghost" icon={<ChevronLeft />} onClick={onBack}>
          {t('skins.catalog.back')}
        </Button>
        {at >= 0 && (
          <p className="look-page-at tabular" role="status">
            {t('skins.catalog.position', { n: at + 1, count: total })}
          </p>
        )}
        <Button icon={<Shuffle />} disabled={total < 2} onClick={onRandom}>
          {t('skins.catalog.random')}
        </Button>
      </header>
      <div className="look-page-body">
        <div className="look-figure">
          <Stage
            ref={stage}
            pixels={figure?.pixels ?? null}
            rev={0}
            model={figure?.look.model ?? look.model}
            hidden={NO_PARTS_HIDDEN}
            paintLayer="base"
            pose={pose}
            straight={false}
            hover={null}
            label={t('skins.catalog.figure', { name })}
            paintable={false}
            onHover={noop}
            onBegin={noop}
            onPaint={noop}
            onEnd={noop}
          />
          <Button size="icon" className="btn-lg look-step look-step-back" icon={<ChevronLeft />} disabled={first} aria-label={t('skins.catalog.previous')} title={t('skins.catalog.previous')} onClick={() => onStep(-1)} />
          <Button size="icon" className="btn-lg look-step look-step-on" icon={<ChevronRight />} disabled={last} aria-label={t('skins.catalog.next')} title={t('skins.catalog.next')} onClick={() => onStep(1)} />
          <Button size="icon" variant="ghost" className="look-pose" icon={<HumanArmsUp />} aria-haspopup="menu" aria-label={poseLabel} title={poseLabel} onClick={(e) => poseMenu.current?.openAt(e.currentTarget)} />
          <Menu
            ref={poseMenu}
            label={t('skins.poseLabel')}
            trigger={false}
            className="contents"
            items={POSES.map((p) => ({ id: p, label: t(`skins.pose.${p}`), icon: p === pose ? <span className="lamp" aria-hidden /> : undefined, onSelect: () => setPose(p) }))}
          />
        </div>
        <div className="look-side">
          <div className="look-about">
            <h2 id={nameId} className="look-name">
              {name}
            </h2>
            <p className="text-sm text-ink-dim">
              {t('skins.catalog.by')} · {t(`skins.model.${look.model}`)}
            </p>
            {tags.length > 0 && (
              <div role="group" aria-label={t('skins.catalog.tags')} className="browse-chips look-tags">
                {tags.map((tag) => (
                  <button key={tag.id} type="button" className="chip" title={t('skins.catalog.seeAllTag', { tag: tag.name })} onClick={() => onTag(tag.id)}>
                    {tag.name}
                  </button>
                ))}
              </div>
            )}
            <WornList label={t('skins.catalog.wears')} groups={wears} lang={lang} />
            <WornList label={t('skins.catalog.base')} groups={base} lang={lang} />
          </div>
          <div className="look-actions">
            {mine ? (
              <p className="look-have" role="status">
                <span className="lamp" aria-hidden />
                {t('skins.catalog.have')}
              </p>
            ) : (
              <Button variant="primary" size="lg" icon={<Plus />} aria-disabled={busy !== null || undefined} onClick={() => keep('add')}>
                {busy === 'add' ? t('skins.catalog.adding') : t('skins.catalog.add')}
              </Button>
            )}
            <Button size="lg" icon={<MagicEdit />} aria-disabled={busy !== null || undefined} onClick={() => keep('customize')}>
              {busy === 'customize' && !mine ? t('skins.catalog.adding') : t('skins.catalog.customize')}
            </Button>
            <p className="text-center text-sm text-ink-dim">{t('skins.catalog.addNote')}</p>
          </div>
        </div>
      </div>
    </section>
  );
}
