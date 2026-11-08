// The Catalog tab: Perland's ready-made looks. Search and tag keys over a grid of looks; the picked one stands on a
// figure at right with the pieces it's made of and the tab's one lit key, Add to my wardrobe.
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { HumanArmsUp } from 'pixelarticons/react';
import { commands } from '../../ipc/commands';
import type { CatalogLook, CatalogManifest, SkinEntry, SkinLook } from '../../ipc/schema';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Plus } from '../../ui/Icon';
import { Menu, type MenuHandle } from '../../ui/Menu';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { searchLooks, type Lang } from './catalogSearch';
import { encodeTexture } from './codec';
import { compose } from './compose';
import { Pixels } from './paint';
import { POSES, type Pose } from './poses';
import { Stage } from './Stage';
import { useCatalog, type Catalog } from './useCatalog';

const NO_PARTS_HIDDEN = new Set<string>();
/** The face every look starts from: not listed among its pieces, which name what it wears. */
const BASE_SLOTS = new Set(['body', 'eyes', 'brows', 'mouth']);
const noop = () => {};

/** A catalog look as a wardrobe look: its pieces with their colours. */
const asLook = (look: CatalogLook): SkinLook => ({ pieces: look.pieces.map((p) => ({ id: p.id, colours: { ...p.colours } })), painting: look.painting !== undefined });

/** The look put together on its own arm model: its painting (when it has one) under its pieces. */
async function composeLook(catalog: Catalog, look: CatalogLook) {
  const [layers, painting] = await Promise.all([catalog.layers(asLook(look), look.model), look.painting ? catalog.pixels(look.painting) : null]);
  return { texels: compose(painting, layers), painting };
}

function LookTile({ look, lang, picked, image, onPick }: { look: CatalogLook; lang: Lang; picked: boolean; image: string | undefined; onPick: () => void }) {
  return (
    <li>
      <button type="button" className="catalog-look" aria-pressed={picked} aria-label={look.name[lang]} onClick={onPick}>
        <span className="catalog-thumb">{image && <img src={image} alt="" draggable={false} />}</span>
        <span className="catalog-look-name">{look.name[lang]}</span>
      </button>
    </li>
  );
}

function LookPreview({ catalog, manifest, look, lang, owner, onAdded }: { catalog: Catalog; manifest: CatalogManifest; look: CatalogLook; lang: Lang; owner: string | null; onAdded: (entry: SkinEntry) => void }) {
  const { t } = useTranslation();
  const [figure, setFigure] = useState<{ id: string; pixels: Pixels } | null>(null);
  const [pose, setPose] = useState<Pose>('still');
  const poseMenu = useRef<MenuHandle>(null);
  const [adding, setAdding] = useState(false);
  const once = useSingleFlight();

  useEffect(() => {
    let live = true;
    composeLook(catalog, look)
      .then(({ texels }) => live && setFigure({ id: look.id, pixels: new Pixels(texels) }))
      .catch(() => live && setFigure(null));
    return () => {
      live = false;
    };
  }, [catalog, look]);

  const tagName = new Map(manifest.tags.map((tag) => [tag.id, tag.name[lang]]));
  // The byline names what kind of look it is; "New" says when, so it comes last.
  const kind = [...look.tags.filter((id) => id !== 'new'), ...look.tags.filter((id) => id === 'new')];
  const firstTag = kind.map((id) => tagName.get(id)).find((n) => n !== undefined);
  const pieceName = new Map(manifest.pieces.filter((p) => !BASE_SLOTS.has(p.slot)).map((p) => [p.id, p.name[lang]]));
  const name = look.name[lang];

  const add = () =>
    once((done) => {
      setAdding(true);
      void composeLook(catalog, look)
        .then(async ({ texels, painting }) => {
          const texture = encodeTexture(texels);
          const entry = await commands.skinCreate(name.slice(0, 64), look.model, texture, owner);
          return commands.skinLookSave(entry.id, asLook(look), painting ? encodeTexture(painting) : null, texture);
        })
        .then(onAdded)
        .catch(pushError)
        .finally(() => {
          setAdding(false);
          done();
        });
    });

  return (
    <section className="catalog-preview" aria-labelledby="catalog-look-name">
      <header className="catalog-preview-head">
        <div className="min-w-0">
          <h2 id="catalog-look-name" className="catalog-preview-name">
            {name}
          </h2>
          <p className="text-sm text-ink-dim">{firstTag ? t('skins.catalog.byTag', { tag: firstTag }) : t('skins.catalog.by')}</p>
        </div>
        <Button size="icon" variant="ghost" icon={<HumanArmsUp />} aria-haspopup="menu" aria-label={`${t('skins.poseLabel')}: ${t(`skins.pose.${pose}`)}`} title={`${t('skins.poseLabel')}: ${t(`skins.pose.${pose}`)}`} onClick={(e) => poseMenu.current?.openAt(e.currentTarget)} />
        <Menu
          ref={poseMenu}
          label={t('skins.poseLabel')}
          trigger={false}
          className="contents"
          items={POSES.map((p) => ({ id: p, label: t(`skins.pose.${p}`), icon: p === pose ? <span className="lamp" aria-hidden /> : undefined, onSelect: () => setPose(p) }))}
        />
      </header>
      <div className="catalog-figure">
        <Stage
          pixels={figure?.id === look.id ? figure.pixels : null}
          rev={0}
          model={look.model}
          hidden={NO_PARTS_HIDDEN}
          paintLayer="base"
          pose={pose}
          paused={false}
          hover={null}
          label={t('skins.catalog.figure', { name })}
          paintable={false}
          onHover={noop}
          onBegin={noop}
          onPaint={noop}
          onEnd={noop}
        />
      </div>
      <ul className="catalog-pieces" aria-label={t('skins.catalog.pieces')}>
        {look.pieces.map((p) => {
          const piece = pieceName.get(p.id);
          return piece ? (
            <li key={p.id} className="tag">
              {piece}
            </li>
          ) : null;
        })}
      </ul>
      <Button variant="primary" size="lg" className="catalog-add" icon={<Plus />} aria-disabled={adding || undefined} onClick={add}>
        {adding ? t('skins.catalog.adding') : t('skins.catalog.add')}
      </Button>
      <p className="text-center text-sm text-ink-dim">{t('skins.catalog.addNote')}</p>
    </section>
  );
}

/** `owner` is the account whose wardrobe is shown (null: the loose skins); an added look goes into it. */
export function CatalogPanel({ owner, onAdded }: { owner: string | null; onAdded: (entry: SkinEntry) => void }) {
  const { t, i18n } = useTranslation();
  const lang: Lang = i18n.language.startsWith('es') ? 'es' : 'en';
  const catalog = useCatalog();
  const { manifest, status } = catalog;
  const [query, setQuery] = useState('');
  const [tag, setTag] = useState('');
  const [picked, setPicked] = useState<string | null>(null);
  const looks = useMemo(() => (manifest ? searchLooks(manifest, query, tag, lang) : []), [manifest, query, tag, lang]);

  if (catalog.loading) {
    return (
      <div className="catalog" aria-busy="true">
        <p className="catalog-message text-ink-dim">{t('skins.catalog.loading')}</p>
      </div>
    );
  }
  if (!manifest) {
    return (
      <div className="catalog">
        <div className="catalog-message">
          <p className="empty-slot">{t('skins.catalog.unavailable')}</p>
          {status === 'too_new' && <p className="info-line">{t('skins.catalog.tooNew')}</p>}
        </div>
      </div>
    );
  }

  // The picked look stays shown while a search hides it; with nothing picked, the first look of the grid is.
  const shown = manifest.looks.find((l) => l.id === picked && !l.retired) ?? looks[0] ?? null;

  return (
    <div className="catalog">
      <div className="catalog-browse">
        <input type="search" className="field-input" aria-label={t('skins.catalog.search')} placeholder={t('skins.catalog.searchPlaceholder')} value={query} onChange={(e) => setQuery(e.target.value)} />
        <div role="group" aria-label={t('skins.catalog.tags')} className="browse-chips">
          <button type="button" className="chip" aria-pressed={tag === ''} onClick={() => setTag('')}>
            {t('skins.catalog.all')}
          </button>
          {manifest.tags.map((tg) => (
            <button key={tg.id} type="button" className="chip" aria-pressed={tag === tg.id} onClick={() => setTag(tg.id)}>
              {tg.name[lang]}
            </button>
          ))}
        </div>
        {status === 'cached' && <p className="text-sm text-ink-dim">{t('skins.catalog.cached')}</p>}
        {status === 'too_new' && <p className="info-line">{t('skins.catalog.tooNew')}</p>}
        <ul className="catalog-grid" aria-label={t('skins.catalog.looks')}>
          {looks.map((look) => (
            <LookTile key={look.id} look={look} lang={lang} picked={look.id === shown?.id} image={catalog.image(look.thumb)} onPick={() => setPicked(look.id)} />
          ))}
        </ul>
        {looks.length === 0 && <p className="empty-slot text-ink-dim">{t('skins.catalog.none')}</p>}
      </div>
      {shown && <LookPreview key={shown.id} catalog={catalog} manifest={manifest} look={shown} lang={lang} owner={owner} onAdded={onAdded} />}
    </div>
  );
}
