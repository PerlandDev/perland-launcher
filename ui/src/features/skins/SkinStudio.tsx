// The Skins screen: a fitting room. The wardrobe at left, the figure on its stage in the middle, the flat texture and
// colours at right, and the tools in a hotbar along the bottom (keys 1–9).
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { open, save } from '@tauri-apps/plugin-dialog';
import { HumanArmsUp, Pause, Play, Redo, Undo, ZoomIn, ZoomOut } from 'pixelarticons/react';
import { commands } from '../../ipc/commands';
import type { SkinEntry, SkinLook, SkinModel, SkinVersion } from '../../ipc/schema';
import { useAccounts } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError, pushNotice } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { Field } from '../../ui/Field';
import { ChevronDown, Clock, Close, Copy, Download, Pencil, Reload, Shirt, Trash } from '../../ui/Icon';
import { Menu, type MenuHandle, type MenuItem } from '../../ui/Menu';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { decodeTexture, encodeTexture } from './codec';
import { ColourPanel } from './ColourPanel';
import { Hotbar } from './Hotbar';
import { POSES, type Pose } from './poses';
import { SLOT_TOOLS } from './tools';
import { PARTS, type Layer } from './layout';
import { Pixels, toHex, type Rgba, type Tool } from './paint';
import { PartsPanel } from './PartsPanel';
import { remember } from './recent';
import { Stage, type StageHandle } from './Stage';
import { blankSkin, starterSkin } from './starter';
import { CatalogPanel } from './CatalogPanel';
import { CreatePanel } from './CreatePanel';
import type { Lang } from './catalogSearch';
import { compose } from './compose';
import type { Focus } from './framing';
import { putOn } from './look';
import { useCatalog } from './useCatalog';
import { surprise } from './surprise';
import { useBlinkFrame } from './useBlink';
import { useLook } from './useLook';
import { useTryOn } from './useTryOn';
import { WearingRow } from './WearingRow';
import { TexturePanel } from './TexturePanel';
import { HistoryPanel } from './HistoryPanel';
import { useSkinDoc } from './useSkinDoc';
import { whenLabel } from './when';
import { Wardrobe } from './Wardrobe';
import './skins.css';

/** One wardrobe per account; `null` is the skins made with no account signed in. */
const skinsKey = (owner: string | null) => ['skins', owner] as const;
const RECENT_MAX = 9;
/** The studio's tabs: build from pieces, paint by hand, or pick a look from Perland's catalog. */
const TABS = ['create', 'paint', 'catalog'] as const;
type Tab = (typeof TABS)[number];

function typing(target: EventTarget | null) {
  return (
    target instanceof HTMLElement &&
    (target instanceof HTMLTextAreaElement || target.isContentEditable || (target instanceof HTMLInputElement && target.type !== 'checkbox' && target.type !== 'radio'))
  );
}

/** What Build from pieces starts wearing, in order. */
const STARTER_PIECES = ['body-plain', 'eyes-round', 'brows-straight', 'mouth-smile'] as const;

export default function SkinStudio({ skin, account }: { skin?: string; account?: string }) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const accounts = useAccounts();
  // Whose wardrobe is shown: the account Edit skin opened, then the one picked here, else the active account.
  const [shown, setShown] = useState<string | undefined>(account);
  const allAccounts = useMemo(() => accounts.data?.accounts ?? [], [accounts.data]);
  const wearer = useMemo(
    () => allAccounts.find((a) => a.uuid === shown) ?? allAccounts.find((a) => a.uuid === accounts.data?.active) ?? null,
    [allAccounts, shown, accounts.data?.active],
  );
  const owner = wearer?.uuid ?? null;
  const SKINS_KEY = skinsKey(owner);
  // A Microsoft wardrobe opens on the skin the account really wears, so it's read (and nothing is picked) only once
  // that's been asked: a list fetched meanwhile would miss it.
  const [ready, setReady] = useState<ReadonlySet<string>>(new Set());
  const waiting = wearer?.provider === 'microsoft' && !ready.has(wearer.uuid);
  // The active account adopts loose skins as it lists them; the starter lets it drop an untouched one instead.
  const adopting = owner !== null && owner === accounts.data?.active;
  const entries = useQuery({
    queryKey: SKINS_KEY,
    queryFn: () => commands.skinsList(owner, adopting ? encodeTexture(starterSkin().data) : null),
    enabled: accounts.isSuccess && !waiting,
  });
  const list = useMemo(() => entries.data ?? [], [entries.data]);
  const [picked, setPicked] = useState<string | null>(skin ?? null);
  const selected = waiting ? null : (list.find((e) => e.id === picked) ?? list[0] ?? null);
  const model: SkinModel = selected?.model ?? 'classic';
  const refresh = () => void qc.invalidateQueries({ queryKey: SKINS_KEY });
  // Stores the open skin's arm model: shown at once, then saved (a refused save puts the wardrobe back as it is).
  const storeModel = (next: SkinModel) => {
    if (!selected) return;
    const id = selected.id;
    qc.setQueryData<SkinEntry[]>(SKINS_KEY, (old) => old?.map((e) => (e.id === id ? { ...e, model: next } : e)));
    commands
      .skinUpdate(id, { model: next })
      .then((entry) => qc.setQueryData<SkinEntry[]>(SKINS_KEY, (old) => old?.map((e) => (e.id === entry.id ? entry : e))))
      .catch((error) => {
        pushError(error);
        refresh();
      });
  };
  // A save answers the entry as stored: every wardrobe list shows it at once, so reopening the skin reads its new look
  // even before the list is fetched again.
  const saved = (entry: SkinEntry) => {
    qc.setQueriesData<SkinEntry[]>({ queryKey: ['skins'] }, (old) => old?.map((e) => (e.id === entry.id ? entry : e)));
    refresh();
  };
  const doc = useSkinDoc(selected?.id ?? null, model, saved, storeModel, selected?.look);
  const catalog = useCatalog();
  const editor = useLook(selected?.id ?? null, model, doc, catalog);
  const lang: Lang = i18n.language.startsWith('es') ? 'es' : 'en';
  const armsLocked = doc.look !== null && !catalog.manifest;
  const catalogManifest = catalog.manifest;

  const [tab, setTab] = useState<Tab>('paint');
  const [tool, setTool] = useState<Tool>('pencil');
  const [color, setColorState] = useState<Rgba>([108, 171, 60, 255]);
  const [opacity, setOpacity] = useState(100);
  const [recent, setRecent] = useState<string[]>([]);
  const [mirror, setMirror] = useState(false);
  const [darken, setDarken] = useState(true);
  const [paintLayer, setPaintLayer] = useState<Layer>('base');
  const [pose, setPose] = useState<Pose>('still');
  // Pausing holds the figure on its current moment, whichever pose it takes next, until it's played again.
  const [paused, setPaused] = useState(false);
  const poseMenu = useRef<MenuHandle>(null);
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  const [hover, setHover] = useState<[number, number] | null>(null);
  const [renaming, setRenaming] = useState<SkinEntry | null>(null);
  const [newName, setNewName] = useState('');
  const [deleting, setDeleting] = useState<SkinEntry | null>(null);
  const [wearing, setWearing] = useState(false);
  // Restoring a version of a skin with pieces asks first: it becomes a plain painted skin.
  const [confirmPlain, setConfirmPlain] = useState(false);
  // Version history: the panel in the texture column, and the version shown on the stage instead of the skin.
  const [historyOpen, setHistoryOpen] = useState(false);
  const [preview, setPreview] = useState<{ skin: string; version: SkinVersion; pixels: Pixels; seq: number } | null>(null);
  const previewSeq = useRef(0);
  const versionsKey = ['skinVersions', selected?.id ?? null] as const;
  const versions = useQuery({ queryKey: versionsKey, queryFn: () => commands.skinVersions(selected!.id), enabled: historyOpen && !!selected });
  const refreshVersions = () => void qc.invalidateQueries({ queryKey: ['skinVersions'] });
  const stage = useRef<StageHandle | null>(null);
  const once = useSingleFlight();
  // A preview belongs to one skin while the history is open; switching skins or closing the history ends it.
  const previewShown = preview && historyOpen && preview.skin === selected?.id ? preview : null;
  const previewing = previewShown !== null;
  // Trying a piece on (Create): the figure shows it, the skin doesn't have it. A version preview comes first.
  const { shown: tryShown, tryOn } = useTryOn(editor, selected?.id ?? null, tab === 'create' && !historyOpen);
  const trying = previewing ? null : tryShown;
  // The figure blinks when the skin wears eyes that show: never over a preview, and never while a stroke is painted.
  const [stroking, setStroking] = useState(false);
  const beginStroke = () => {
    setStroking(true);
    doc.begin();
  };
  const endStroke = () => {
    setStroking(false);
    doc.end();
  };
  const blinkFrame = useBlinkFrame(selected?.id ?? null, doc, catalog, model, tab !== 'catalog' && !previewing && !trying && !stroking);
  // What the Create tab is dressing: the stage camera frames it. Any other tab, and History, show the whole figure.
  const [dressing, setDressing] = useState<{ focus: Focus; category: string }>({ focus: 'figure', category: '' });
  const onCreateFocus = useCallback((focus: Focus, category: string) => setDressing((d) => (d.focus === focus && d.category === category ? d : { focus, category })), []);
  const framing = tab === 'create' && !historyOpen && selected ? { focus: dressing.focus, key: `create:${selected.id}:${dressing.category}` } : { focus: 'figure' as const, key: tab };

  /** Chooses a colour for good: a new one joins the front of the recent row, one already in it stays put. */
  const setColor = (c: Rgba) => {
    setColorState(c);
    const hex = toHex(c);
    setRecent((r) => remember(r, hex, RECENT_MAX));
  };

  const create = async (name: string, model: SkinModel, texture: string, seeded = false) => {
    const entry = await commands.skinCreate(name, model, texture, owner, seeded);
    qc.setQueryData<SkinEntry[]>(SKINS_KEY, (old) => [entry, ...(old ?? [])]);
    setPicked(entry.id);
    return entry;
  };

  const fromAccount = async () => {
    if (!wearer) return;
    const current = await commands.skinCurrent(wearer.uuid);
    if (!current) {
      pushNotice({ title: t('skins.noCurrent') });
      return;
    }
    await create(t('skins.currentName', { name: wearer.username }), current.model, current.texture);
  };

  // Every time a Microsoft account's wardrobe is shown (opening the screen, adding or switching accounts), the skin it
  // really wears is put in it (once: a matching entry is reused) and opened. One wearing Minecraft's default gets no
  // made-up skin, and an empty wardrobe says so.
  const synced = useRef(new Set<string>());
  const [defaults, setDefaults] = useState<ReadonlySet<string>>(new Set());
  useEffect(() => {
    if (wearer?.provider !== 'microsoft' || synced.current.has(wearer.uuid)) return;
    const { uuid, username } = wearer;
    synced.current.add(uuid);
    void commands
      .skinSyncCurrent(uuid, t('skins.currentName', { name: username }))
      .then(async (entry) => {
        if (entry) {
          await qc.invalidateQueries({ queryKey: skinsKey(uuid) });
          setPicked(entry.id);
        } else setDefaults((d) => new Set(d).add(uuid));
      })
      .catch(pushError)
      .finally(() => setReady((r) => new Set(r).add(uuid)));
  }, [wearer, qc, t]);

  // An offline account's empty wardrobe, or the one used with no account, starts from the Perlander, once per wardrobe.
  const seeded = useRef(new Set<string | null>());
  useEffect(() => {
    if (!entries.isSuccess || list.length > 0 || wearer?.provider === 'microsoft' || seeded.current.has(owner)) return;
    seeded.current.add(owner);
    // Seeded: left untouched, it isn't carried into the wardrobe of the account the player signs in with next.
    create(t('skins.starterName'), 'classic', encodeTexture(starterSkin().data), true).catch(pushError);
    // create and t are stable enough for a one-time seed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries.isSuccess, list.length, wearer, owner]);
  const defaultSkin = wearer !== null && defaults.has(wearer.uuid);

  // Saves now, after any Create change still loading has landed, so leaving, wearing or exporting keeps it.
  const flush = () => editor.settled().then(() => doc.flush());

  const showWardrobeOf = (uuid: string) =>
    void flush().then(() => {
      setShown(uuid);
      setPicked(null);
    });

  // A new skin wearing the catalog's plain body and a simple face (round eyes, straight brows, a smile), each in its own
  // colours, opened in Create. A face piece the catalog no longer has is left out; without the body there is nothing.
  const plainBody = catalog.manifest?.pieces.find((p) => p.id === 'body-plain' && !p.retired);
  const buildFromPieces = async () => {
    const manifest = catalog.manifest;
    if (!plainBody || !manifest) return;
    const starting = STARTER_PIECES.flatMap((id) => manifest.pieces.filter((p) => p.id === id && !p.retired));
    const look = starting.reduce((l, piece) => putOn(l, piece, manifest.pieces), { pieces: [], painting: false } as SkinLook);
    const texture = encodeTexture(compose(null, await catalog.layers(look, 'classic')));
    const entry = await commands.skinCreate(t('skins.piecesName'), 'classic', texture, owner);
    addedFromCatalog(await commands.skinLookSave(entry.id, look, null, texture));
  };

  const newItems: MenuItem[] = [
    ...(wearer?.provider === 'microsoft'
      ? [{ id: 'current', label: t('skins.newFromAccount'), onSelect: () => void fromAccount().catch(pushError) }]
      : []),
    { id: 'starter', label: t('skins.newStarter'), onSelect: () => void create(t('skins.starterName'), 'classic', encodeTexture(starterSkin().data)).catch(pushError) },
    { id: 'blank', label: t('skins.newBlank'), onSelect: () => void create(t('skins.blankName'), 'classic', encodeTexture(blankSkin().data)).catch(pushError) },
    ...(plainBody ? [{ id: 'pieces', label: t('skins.newPieces'), onSelect: () => void buildFromPieces().catch(pushError) }] : []),
  ];

  const importSkin = async () => {
    const path = await open({ multiple: false, filters: [{ name: t('skins.pngFilter'), extensions: ['png'] }] });
    if (typeof path !== 'string') return;
    const entry = await commands.skinImport(path, owner);
    qc.setQueryData<SkinEntry[]>(SKINS_KEY, (old) => [entry, ...(old ?? [])]);
    setPicked(entry.id);
  };

  const exportSkin = async (entry: SkinEntry) => {
    await flush();
    const path = await save({ defaultPath: `${entry.name.replace(/[<>:"/\\|?*]/g, '-')}.png`, filters: [{ name: t('skins.pngFilter'), extensions: ['png'] }] });
    if (!path) return;
    await commands.skinExport(entry.id, path);
    refreshVersions();
    pushNotice({ title: t('skins.exported', { name: entry.name }) });
  };

  const menuFor = (entry: SkinEntry): MenuItem[] => [
    { id: 'rename', label: t('skins.rename'), icon: <Pencil />, onSelect: () => (setNewName(entry.name), setRenaming(entry)) },
    {
      id: 'duplicate',
      label: t('skins.duplicate'),
      icon: <Copy />,
      onSelect: () =>
        void flush()
          .then(() => commands.skinDuplicate(entry.id, t('skins.copyName', { name: entry.name }).slice(0, 64)))
          .then((copy) => {
            qc.setQueryData<SkinEntry[]>(SKINS_KEY, (old) => [copy, ...(old ?? [])]);
            setPicked(copy.id);
          })
          .catch(pushError),
    },
    { id: 'export', label: t('skins.export'), icon: <Download />, onSelect: () => void exportSkin(entry).catch(pushError) },
    { id: 'delete', label: t('skins.delete'), icon: <Trash />, danger: true, onSelect: () => setDeleting(entry) },
  ];


  const showLayer = (layer: Layer, show: boolean) =>
    setHidden((h) => {
      const next = new Set(h);
      for (const p of PARTS) {
        if (show) next.delete(`${p}:${layer}`);
        else next.add(`${p}:${layer}`);
      }
      return next;
    });
  const togglePart = (key: string) =>
    setHidden((h) => {
      const next = new Set(h);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const strokeOpts = { tool, color, mirror, darken, opacity: opacity / 100 };
  const paintAt = (x: number, y: number, joined: boolean) => {
    if (previewing || tab !== 'paint') return;
    if (tool === 'picker') {
      const c = doc.pixels?.get(x, y);
      if (c && c[3] > 0) setColor([c[0], c[1], c[2], 255]);
      return;
    }
    doc.paint(x, y, strokeOpts, joined);
  };

  const wear = () => {
    if (!selected || !wearer) return;
    once((done) => {
      setWearing(true);
      void flush()
        .then(() => commands.skinWear(selected.id, wearer.uuid))
        .then((next) => {
          qc.setQueryData(SKINS_KEY, next);
          refreshVersions();
          void qc.invalidateQueries({ queryKey: ['accountFace'] });
          stage.current?.celebrate();
          pushNotice({ title: t('skins.worn', { name: selected.name }), body: t('skins.wornBody') });
        })
        .catch(pushError)
        .finally(() => {
          setWearing(false);
          done();
        });
    });
  };

  const pickVersion = (version: SkinVersion) => {
    if (!selected) return;
    const skin = selected.id;
    void flush()
      .then(() => commands.skinVersionTexture(skin, version.id))
      .then(decodeTexture)
      .then((data) => setPreview({ skin, version, pixels: new Pixels(data), seq: (previewSeq.current += 1) }))
      .catch(pushError);
  };

  const restore = () => {
    if (!selected || !previewShown) return;
    if (doc.look) {
      setConfirmPlain(true);
      return;
    }
    restoreNow();
  };

  const restoreNow = () => {
    setConfirmPlain(false);
    if (!selected || !previewShown) return;
    const { version, pixels } = previewShown;
    once((done) => {
      void flush()
        .then(() => commands.skinRestore(selected.id, version.id))
        .then((entry) => {
          qc.setQueryData<SkinEntry[]>(SKINS_KEY, (old) => old?.map((e) => (e.id === entry.id ? entry : e)));
          doc.replace(pixels.data);
          setPreview(null);
          refreshVersions();
          pushNotice({ title: t('skins.history.restored'), body: t('skins.history.restoredBody', { when: whenLabel(version.taken, i18n.language, t) }) });
        })
        .catch(pushError)
        .finally(done);
    });
  };

  const copyVersion = () => {
    if (!selected || !previewShown) return;
    const { version } = previewShown;
    const name = t('skins.history.copyName', { name: selected.name, when: whenLabel(version.taken, i18n.language, t) }).slice(0, 64);
    once((done) => {
      void commands
        .skinVersionCopy(selected.id, version.id, name)
        .then((copy) => {
          qc.setQueryData<SkinEntry[]>(SKINS_KEY, (old) => [copy, ...(old ?? [])]);
          setHistoryOpen(false);
          setPicked(copy.id);
        })
        .catch(pushError)
        .finally(done);
    });
  };

  // Keys: 1–9 press the hotbar, Ctrl+Z undoes, Ctrl+Shift+Z or Ctrl+Y redoes, Shift held makes Shade lighten.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.type === 'keyup') {
        if (event.key === 'Shift') setDarken(true);
        return;
      }
      if (event.key === 'Shift') setDarken(false);
      if (tab === 'catalog' || typing(event.target) || document.querySelector('dialog[open]') || event.altKey) return;
      if (event.key === 'Escape' && previewing) {
        setPreview(null);
        return;
      }
      if (event.key === 'Escape' && trying) {
        tryOn.clear();
        return;
      }
      const ctrl = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();
      if (ctrl && (key === 'y' || (key === 'z' && event.shiftKey))) {
        event.preventDefault();
        doc.redo();
      } else if (ctrl && key === 'z') {
        event.preventDefault();
        doc.undo();
      } else if (!ctrl && tab === 'paint' && /^[1-9]$/.test(event.key)) {
        const n = Number(event.key);
        if (n <= SLOT_TOOLS.length) setTool(SLOT_TOOLS[n - 1]!);
        else if (n === 7) setMirror((m) => !m);
        else if (n === 8) doc.undo();
        else doc.redo();
      }
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('keyup', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('keyup', onKey);
    };
  }, [doc, previewing, tab, trying, tryOn]);

  // A look added from the catalog is opened in the wardrobe it went into, and in Create unless the player is still
  // browsing the catalog (`customize` false).
  const addedFromCatalog = (entry: SkinEntry, customize = true) =>
    void flush().then(() => {
      qc.setQueryData<SkinEntry[]>(SKINS_KEY, (old) => [entry, ...(old ?? []).filter((e) => e.id !== entry.id)]);
      setPicked(entry.id);
      if (customize) setTab('create');
    });

  const saveLabel = doc.save === 'saved' ? t('skins.saved') : t('skins.saving');

  return (
    <section className="skins-screen" aria-labelledby="skins-title">
      <header className="skins-head">
        <Button variant="ghost" size="icon" aria-label={t('common.back')} title={t('common.back')} icon={<ChevronDown className="rotate-90" />} onClick={() => navigate({ name: 'home', id: null })} />
        <h1 id="skins-title" className="font-display text-[2rem] leading-none">
          {t('skins.title')}
        </h1>
        <div role="tablist" aria-labelledby="skins-title" className="skins-tabs">
          {TABS.map((id) => (
            <button key={id} type="button" role="tab" id={`skins-tab-${id}`} aria-selected={tab === id} aria-controls="skins-tabpanel" className="seg" data-checked={tab === id || undefined} onClick={() => setTab(id)}>
              {t(`skins.tab.${id}`)}
            </button>
          ))}
        </div>
        {selected && tab !== 'catalog' && (
          <p className="min-w-0 truncate text-lg font-bold" title={selected.name}>
            {selected.name}
          </p>
        )}
        <p className="ml-auto text-sm text-ink-dim" role="status">
          {selected && tab !== 'catalog' ? saveLabel : ''}
        </p>
        {tab !== 'catalog' && (
          <Button variant="ghost" icon={<Clock />} aria-pressed={historyOpen} disabled={!selected} onClick={() => setHistoryOpen((o) => !o)}>
            {t('skins.history.title')}
          </Button>
        )}
      </header>

      <div className="skins-body" data-tab={tab} role="tabpanel" id="skins-tabpanel" aria-labelledby={`skins-tab-${tab}`}>
        <Wardrobe
          accounts={allAccounts}
          owner={wearer}
          onOwner={showWardrobeOf}
          entries={list}
          selected={selected?.id ?? null}
          wearer={wearer?.uuid ?? null}
          onSelect={(id) => void flush().then(() => setPicked(id))}
          emptyNote={defaultSkin ? t('skins.emptyDefault') : t('skins.empty')}
          newItems={newItems}
          onImport={() => void importSkin().catch(pushError)}
          menuFor={menuFor}
        />

        {tab === 'catalog' ? (
          <CatalogPanel owner={owner} onAdded={(entry) => addedFromCatalog(entry, false)} onCustomize={(entry) => addedFromCatalog(entry)} />
        ) : (
          <>
            <div className="stage">
              <div className="stage-bar">
                <div role="radiogroup" aria-label={t('skins.arms')} className="flex gap-2">
                  {(['classic', 'slim'] as const).map((m) => (
                    <button
                      key={m}
                      type="button"
                      role="radio"
                      aria-checked={model === m}
                      className="seg seg-sm"
                      data-checked={model === m || undefined}
                      // A skin wearing pieces needs the catalog to draw them for the other arms.
                      disabled={armsLocked}
                      title={armsLocked ? t('skins.create.armsNeedCatalog') : undefined}
                      onClick={() => selected && (doc.look ? editor.switchModel(m) : doc.switchModel(m))}
                    >
                      {t(`skins.model.${m}`)}
                    </button>
                  ))}
                </div>
                <Button icon={<HumanArmsUp />} aria-haspopup="menu" aria-label={`${t('skins.poseLabel')}: ${t(`skins.pose.${pose}`)}`} title={t('skins.poseLabel')} onClick={(e) => poseMenu.current?.openAt(e.currentTarget)}>
                  <span className="btn-label">{t(`skins.pose.${pose}`)}</span>
                </Button>
                <Menu
                  ref={poseMenu}
                  label={t('skins.poseLabel')}
                  trigger={false}
                  className="contents"
                  items={POSES.map((p) => ({ id: p, label: t(`skins.pose.${p}`), icon: p === pose ? <span className="lamp" aria-hidden /> : undefined, onSelect: () => setPose(p) }))}
                />
                <Button icon={paused ? <Play /> : <Pause />} title={paused ? t('skins.playMotion') : t('skins.pauseMotion')} onClick={() => setPaused((p) => !p)}>
                  <span className="btn-label">{paused ? t('skins.playMotion') : t('skins.pauseMotion')}</span>
                </Button>
                {tab === 'create' && (
                  <div className="ml-auto flex gap-2">
                    <Button variant="ghost" size="icon" aria-label={t('skins.undo')} title={t('skins.undo')} icon={<Undo />} disabled={!doc.canUndo} onClick={doc.undo} />
                    <Button variant="ghost" size="icon" aria-label={t('skins.redo')} title={t('skins.redo')} icon={<Redo />} disabled={!doc.canRedo} onClick={doc.redo} />
                  </div>
                )}
              </div>

              <div className="stage-well">
                <Stage
                  pixels={previewing ? previewShown.pixels : trying ? trying.pixels : doc.pixels}
                  // A preview gets its own (negative) revisions, so the figure redraws on every switch.
                  rev={previewing ? -previewShown.seq : trying ? -1e6 - trying.seq : doc.rev}
                  model={model}
                  hidden={hidden}
                  paintLayer={paintLayer}
                  pose={pose}
                  paused={paused}
                  hover={hover}
                  onHover={setHover}
                  onBegin={beginStroke}
                  onPaint={(x, y) => paintAt(x, y, false)}
                  onEnd={endStroke}
                  paintable={tab === 'paint'}
                  label={tab === 'paint' ? undefined : t('skins.stageLabelTurn')}
                  focus={framing.focus}
                  focusKey={framing.key}
                  blink={blinkFrame}
                  blinkSeed={selected?.id ?? ''}
                  ref={stage}
                />
                <div className="stage-view">
                  <Button variant="ghost" size="icon" aria-label={t('skins.zoomIn')} title={t('skins.zoomIn')} icon={<ZoomIn />} onClick={() => stage.current?.zoom(1 / 1.15)} />
                  <Button variant="ghost" size="icon" aria-label={t('skins.zoomOut')} title={t('skins.zoomOut')} icon={<ZoomOut />} onClick={() => stage.current?.zoom(1.15)} />
                  <Button variant="ghost" size="icon" aria-label={t('skins.resetView')} title={t('skins.resetView')} icon={<Reload />} onClick={() => stage.current?.resetView()} />
                </div>
                {previewing ? (
                  <div className="stage-preview" role="group" aria-label={t('skins.history.previewing', { when: whenLabel(previewShown.version.taken, i18n.language, t) })}>
                    <p className="stage-preview-note">
                      <span className="preview-long truncate font-bold">{t('skins.history.previewing', { when: whenLabel(previewShown.version.taken, i18n.language, t) })}</span>
                      <span className="preview-short truncate font-bold tabular">{whenLabel(previewShown.version.taken, i18n.language, t)}</span>
                      <span className="preview-long text-ink-dim">{t('skins.history.previewNote')}</span>
                    </p>
                    <div className="stage-preview-keys">
                      <Button variant="ghost" icon={<Close />} title={t('skins.history.back')} onClick={() => setPreview(null)}>
                        <span className="btn-label">{t('skins.history.back')}</span>
                      </Button>
                      <Button icon={<Copy />} title={t('skins.history.copy')} onClick={copyVersion}>
                        <span className="btn-label">{t('skins.history.copy')}</span>
                      </Button>
                      <Button variant="primary" icon={<Reload />} disabled={previewShown.version.current} aria-label={t('skins.history.restore')} title={t('skins.history.restore')} onClick={restore}>
                        <span className="preview-long">{t('skins.history.restore')}</span>
                        <span className="preview-short">{t('skins.history.restoreShort')}</span>
                      </Button>
                    </div>
                  </div>
                ) : (
                  <p className="stage-hint text-sm text-ink-dim">{trying ? t('skins.create.tryHint') : tab === 'paint' ? t('skins.stageHint') : t('skins.stageHintTurn')}</p>
                )}
                {trying && (
                  <p className="stage-try" role="status" data-testid="stage-try">
                    <span className="stage-try-tag">{t('skins.create.trying')}</span>
                    <span className="truncate">{trying.name}</span>
                  </p>
                )}
                <div className="stage-wear" hidden={previewing}>
                  {wearer?.provider === 'microsoft' ? (
                    <Button variant="primary" size="lg" icon={<Shirt />} aria-disabled={wearing || undefined} disabled={!selected} onClick={wear}>
                      {wearing ? t('skins.wearingNow') : t('skins.wear')}
                    </Button>
                  ) : (
                    <p className="stage-offline">{wearer ? t('skins.offlineNote') : t('skins.noAccountNote')}</p>
                  )}
                </div>
              </div>
            </div>

            {historyOpen && selected ? (
              <HistoryPanel skin={selected.id} model={model} hasLook={doc.look !== null} versions={versions.data} picked={previewShown?.version.id ?? null} onPick={pickVersion} onClose={() => setHistoryOpen(false)} />
            ) : tab === 'create' ? (
              <CreatePanel
                key={selected?.id ?? ''}
                catalog={catalog}
                look={editor.look}
                editor={editor}
                lang={lang}
                onBuildFromPieces={plainBody ? () => void buildFromPieces().catch(pushError) : undefined}
                tryOn={tryOn}
                onFocus={onCreateFocus}
              />
            ) : (
            <aside className="skins-side" aria-label={t('skins.texture')}>
              <div className="flex items-center justify-between gap-2">
                <h2 className="sr-only">{t('skins.texture')}</h2>
                <span className="text-sm text-ink-dim" aria-hidden>
                  {t('skins.paintOn')}
                </span>
                <div role="radiogroup" aria-label={t('skins.paintOn')} className="flex items-center gap-2">
                  {(['base', 'outer'] as const).map((layer) => (
                    <button key={layer} type="button" role="radio" aria-checked={paintLayer === layer} title={`${t('skins.paintOn')}: ${t(`skins.layer.${layer}`)}`} className="seg seg-sm" data-checked={paintLayer === layer || undefined} onClick={() => setPaintLayer(layer)}>
                      {t(`skins.layer.${layer}`)}
                    </button>
                  ))}
                </div>
              </div>
              <TexturePanel
                pixels={doc.pixels}
                rev={doc.rev}
                model={model}
                hover={hover}
                paintLayer={paintLayer}
                onHover={setHover}
                onBegin={beginStroke}
                onPaint={paintAt}
                onEnd={endStroke}
              />
              {doc.covered && (
                <p className="paint-covered" role="status">
                  {t('skins.piecesOnTop')}
                </p>
              )}
              <ColourPanel color={color} opacity={opacity} recent={recent} onColor={setColor} onPreview={setColorState} onOpacity={setOpacity} />
              <PartsPanel hidden={hidden} onToggle={togglePart} onLayer={showLayer} />
            </aside>
            )}
          </>
        )}
      </div>

      {tab === 'create' && (
        <WearingRow
          worn={editor.look?.pieces ?? []}
          pieces={catalog.manifest?.pieces ?? null}
          loading={catalog.loading}
          lang={lang}
          onTakeOff={editor.takeOff}
          // The outfit is worked out when its turn comes, from the look as it is then.
          onSurprise={catalogManifest && selected ? () => editor.restyle((look, arms) => surprise(catalogManifest, look, Math.random, arms)) : undefined}
        />
      )}

      {tab === 'paint' && (
        <Hotbar
          tool={tool}
          onTool={setTool}
          mirror={mirror}
          onMirror={() => setMirror((m) => !m)}
          canUndo={doc.canUndo}
          canRedo={doc.canRedo}
          onUndo={doc.undo}
          onRedo={doc.redo}
        />
      )}

      <Dialog
        open={renaming !== null}
        onClose={() => setRenaming(null)}
        title={t('skins.renameTitle')}
        actions={
          <>
            <Button variant="ghost" onClick={() => setRenaming(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              disabled={newName.trim().length === 0}
              onClick={() => {
                const target = renaming;
                if (!target) return;
                commands
                  .skinUpdate(target.id, { name: newName })
                  .then((entry) => qc.setQueryData<SkinEntry[]>(SKINS_KEY, (old) => old?.map((e) => (e.id === entry.id ? entry : e))))
                  .catch(pushError)
                  .finally(() => setRenaming(null));
              }}
            >
              {t('skins.renameSave')}
            </Button>
          </>
        }
      >
        <Field label={t('skins.name')}>
          <input className="field-input" value={newName} maxLength={64} onChange={(e) => setNewName(e.target.value)} />
        </Field>
      </Dialog>

      <Dialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        title={t('skins.deleteTitle', { name: deleting?.name ?? '' })}
        actions={
          <>
            <Button variant="ghost" onClick={() => setDeleting(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                const target = deleting;
                if (!target) return;
                commands
                  .skinDelete(target.id)
                  .then(() => {
                    qc.setQueryData<SkinEntry[]>(SKINS_KEY, (old) => old?.filter((e) => e.id !== target.id));
                    if (picked === target.id) setPicked(null);
                  })
                  .catch(pushError)
                  .finally(() => setDeleting(null));
              }}
            >
              {t('skins.deleteConfirm')}
            </Button>
          </>
        }
      >
        <p className="text-ink-dim">{t('skins.deleteBody')}</p>
      </Dialog>

      <Dialog
        open={confirmPlain}
        onClose={() => setConfirmPlain(false)}
        title={t('skins.history.plainTitle')}
        actions={
          <>
            <Button variant="ghost" onClick={() => setConfirmPlain(false)}>
              {t('common.cancel')}
            </Button>
            <Button variant="primary" icon={<Reload />} onClick={restoreNow}>
              {t('skins.history.plainConfirm')}
            </Button>
          </>
        }
      >
        <p className="text-ink-dim text-pretty">{t('skins.history.plainBody')}</p>
      </Dialog>
    </section>
  );
}
