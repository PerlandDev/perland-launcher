import { useQueryClient } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';
import { useId, useState, type KeyboardEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { InstanceIcon } from '../../art/InstanceIcon';
import { Marquee } from '../../art/Marquee';
import { artSeed, presetSeeds } from '../../art/seed';
import { pixelUpscale } from '../../art/upscale';
import type { ArtChoice, BannerView, IconChoice, Instance, InstanceListing } from '../../ipc/schema';
import {
  qk,
  useInstanceArt,
  useInstanceScreenshot,
  useInstanceScreenshots,
  usePreviewBannerView,
  useSetBannerView,
  useSetInstanceArt,
  useSetInstanceIcon,
} from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Crop, Package, Upload } from '../../ui/Icon';
import { BannerAdjust } from './BannerAdjust';
import { isPack } from './labels';

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp'];
/** The icon slot on the picture keys, in px. */
const SLOT = 32;

/** Which background is in use now (ruling R4): an uploaded picture, the pack's own art, or a landscape's
 *  seed (which lights no tile when it isn't one of the six). */
function currentBackground(instance: Instance): 'upload' | 'pack' | { seed: string } {
  if (instance.art === 'custom') return 'upload';
  if (instance.art === 'auto' && isPack(instance)) return 'pack';
  return { seed: artSeed(instance) };
}

/** Which icon is in use now. A never-chosen icon follows the core: a round-1 upload still fills it, and
 *  otherwise it is on auto, which is the pack's icon for a pack and the initials for anything else. A pack
 *  with no icon of its own (`packIcon` null, once the art has loaded) shows the initials, as the rail does. */
function currentIcon(instance: Instance, packIcon: string | null | undefined): 'upload' | 'pack' | 'generated' {
  const mode = instance.icon ?? (instance.art === 'custom' ? 'custom' : 'auto');
  if (mode === 'custom') return 'upload';
  if (mode === 'auto' && isPack(instance) && packIcon !== null) return 'pack';
  return 'generated';
}

/** Screenshots shown before Show all. */
const FIRST_SCREENSHOTS = 6;

/** A screenshot's name as the player reads it: the game names them by date, so the name without its extension. */
function screenshotLabel(name: string): string {
  return name.replace(/\.(png|jpe?g|webp)$/i, '');
}

/** One of the instance's screenshots, read only once its tile shows. A screenshot that can't be used (not a
 *  real picture, or gone since the list was read) leaves no tile. */
function ScreenshotTile({
  id,
  name,
  current,
  tabbable,
  busy,
  onChoose,
}: {
  id: string;
  name: string;
  current: boolean;
  tabbable: boolean;
  busy: boolean;
  onChoose: () => void;
}) {
  const { t } = useTranslation();
  const picture = useInstanceScreenshot(id, name);
  const [failed, setFailed] = useState(false);
  if (picture.data === null || failed) return null;
  return (
    <button
      type="button"
      role="radio"
      className="art-tile"
      aria-label={t('image.screenshot', { name: screenshotLabel(name) })}
      aria-checked={current}
      aria-disabled={busy || undefined}
      aria-busy={picture.isPending || undefined}
      tabIndex={tabbable ? 0 : -1}
      onClick={onChoose}
    >
      {picture.data && (
        <img src={picture.data} alt="" draggable={false} className="absolute inset-0 h-full w-full object-cover" onError={() => setFailed(true)} />
      )}
    </button>
  );
}

const STEPS: Partial<Record<string, number>> = {
  ArrowRight: 1,
  ArrowDown: 1,
  ArrowLeft: -1,
  ArrowUp: -1,
};

/** A labelled group of choices, one tab stop: arrow keys, Home and End move between them, and Space or
 *  Enter picks. Moving never picks, because every pick saves at once and an Upload key opens the file picker.
 *  `between` sits under the heading, outside the choices (Background's tabs). */
function ChoiceGroup({
  label,
  busy,
  between,
  children,
}: {
  label: string;
  busy: boolean;
  between?: (labelId: string) => ReactNode;
  children: ReactNode;
}) {
  const id = useId();
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const radios = [...event.currentTarget.querySelectorAll<HTMLElement>('[role="radio"]')];
    const at = radios.findIndex((radio) => radio === document.activeElement);
    if (at < 0) return;
    const step = STEPS[event.key];
    const next = step !== undefined ? (at + step + radios.length) % radios.length : event.key === 'Home' ? 0 : event.key === 'End' ? radios.length - 1 : null;
    if (next === null) return;
    event.preventDefault();
    radios[next]?.focus();
  };
  const heading = (
    <h3 id={id} className="art-section-label">
      {label}
    </h3>
  );
  return (
    <div className="art-section">
      {heading}
      {between?.(id)}
      <div role="radiogroup" aria-labelledby={id} aria-busy={busy || undefined} className="art-choices" onKeyDown={onKeyDown}>
        {children}
      </div>
    </div>
  );
}

/** Background's tabs: the generated landscapes and the instance's screenshots. */
type BackgroundTab = 'landscapes' | 'shots';

/** The tabs over Background's choices: one tab stop, the arrow keys move and show at once (they only show, never pick). */
function BackgroundTabs({
  tabs,
  shown,
  labelId,
  panelId,
  onShow,
}: {
  tabs: { id: BackgroundTab; label: string }[];
  shown: BackgroundTab;
  labelId: string;
  panelId: string;
  onShow: (tab: BackgroundTab) => void;
}) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const at = tabs.findIndex((tab) => tab.id === shown);
    const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : null;
    const next = step !== null ? (at + step + tabs.length) % tabs.length : event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : null;
    if (next === null) return;
    event.preventDefault();
    const tab = tabs[next]!;
    onShow(tab.id);
    event.currentTarget.querySelector<HTMLElement>(`[data-tab="${tab.id}"]`)?.focus();
  };
  return (
    <div role="tablist" aria-labelledby={labelId} className="art-tabs" onKeyDown={onKeyDown}>
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          role="tab"
          id={`${panelId}-${tab.id}`}
          data-tab={tab.id}
          className="seg seg-sm"
          data-checked={tab.id === shown ? '' : undefined}
          aria-selected={tab.id === shown}
          aria-controls={panelId}
          tabIndex={tab.id === shown ? 0 : -1}
          onClick={() => onShow(tab.id)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}

/** A picture-choice key: a raised key with its icon slot, pressed into its slot with the lime lamp while in use. */
function PictureKey({
  label,
  hint,
  glyph,
  fill = false,
  picture,
  current,
  tabbable,
  busy,
  onChoose,
  action = false,
}: {
  label: string;
  /** A second, dim line under the label (what an upload may be). It describes the key; the label alone names it. */
  hint?: string;
  /** An action beside a group rather than one of its choices: a plain button, pressed while its picture is in use. */
  action?: boolean;
  glyph: ReactNode;
  /** The glyph is a picture of its own (the initials) and fills the slot. */
  fill?: boolean;
  /** The picture in use (a data: URL) when this is the current choice; otherwise the glyph shows. */
  picture: string | null;
  current: boolean;
  tabbable: boolean;
  busy: boolean;
  onChoose: () => void;
}) {
  const id = useId();
  const [failed, setFailed] = useState<string | null>(null);
  const [upscaled, setUpscaled] = useState<string | null>(null);
  const shown = current && picture && picture !== failed ? picture : null;
  return (
    <button
      type="button"
      role={action ? undefined : 'radio'}
      className="art-key"
      data-action={action || undefined}
      aria-checked={action ? undefined : current}
      aria-pressed={action ? current : undefined}
      aria-disabled={busy || undefined}
      aria-labelledby={`${id}-label`}
      aria-describedby={hint ? `${id}-hint` : undefined}
      tabIndex={action || tabbable ? 0 : -1}
      onClick={onChoose}
    >
      <span className="art-key-slot" data-fill={fill && !shown ? '' : undefined} aria-hidden>
        {shown ? (
          <img
            src={shown}
            alt=""
            width={SLOT}
            height={SLOT}
            draggable={false}
            data-upscale={upscaled === shown ? '' : undefined}
            onLoad={(e) => {
              const { naturalWidth: width, naturalHeight: height } = e.currentTarget;
              setUpscaled(pixelUpscale({ width, height }, { width: SLOT, height: SLOT }) ? shown : null);
            }}
            onError={() => setFailed(shown)}
          />
        ) : (
          glyph
        )}
      </span>
      <span className="art-key-text">
        <span id={`${id}-label`}>{label}</span>
        {hint && (
          <span id={`${id}-hint`} className="art-key-hint">
            {hint}
          </span>
        )}
      </span>
    </button>
  );
}

/** "Change image…": two groups in one dialog. Icon: the initials, the pack's icon (packs only) or an uploaded picture.
 *  Background: two tabs, the six generated landscapes and the instance's screenshots, with Upload beside them. A modpack's
 *  own art stays its background until another is picked. A choice applies at once; the lit key follows it, and the rail
 *  and marquee behind the dialog change with it. While either saves, both groups wait: two saves at once would race on
 *  instance.json. With a picture background, Adjust… turns the same dialog into "Adjust the background" (`BannerAdjust`). */
export function ImageDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const { t } = useTranslation();
  const setArt = useSetInstanceArt({ inline: true });
  const setIcon = useSetInstanceIcon({ inline: true });
  const setView = useSetBannerView({ inline: true });
  const previewView = usePreviewBannerView();
  const [step, setStep] = useState<'choose' | 'adjust'>('choose');
  const art = useInstanceArt(instance.id);
  const screenshots = useInstanceScreenshots(instance.id);
  const [allScreenshots, setAllScreenshots] = useState(false);
  // The screenshot picked here, while it is still the background. The core keeps it as the player's own picture,
  // so the dialog opened again lights Upload for it.
  const [shot, setShot] = useState<string | null>(null);
  const [pickError, setPickError] = useState<unknown>(null);
  const qc = useQueryClient();
  // A refused view starts the adjust view again from what is saved now. The cache has it before this dialog's
  // `instance` does.
  const [restart, setRestart] = useState<{ n: number; instance: Instance } | null>(null);
  const restartView = () =>
    setRestart((last) => ({
      n: (last?.n ?? 0) + 1,
      instance: qc.getQueryData<InstanceListing>(qk.instances)?.instances.find((i) => i.id === instance.id) ?? instance,
    }));
  const pack = isPack(instance);
  const background = currentBackground(instance);
  const icon = currentIcon(instance, art.data && art.data.icon);
  const pictureBusy = setArt.isPending || setIcon.isPending;
  const busy = pictureBusy || setView.isPending;
  // The icon and the background come back apart, each shown on its own current key.
  const iconPicture = art.data?.icon ?? null;
  const bannerPicture = art.data?.banner ?? null;
  const landscapeLit = typeof background === 'object' && presetSeeds(instance).includes(background.seed);
  const shotLit = background === 'upload' && shot !== null && (screenshots.data ?? []).includes(shot);
  const uploadLit = background === 'upload' && !shotLit;
  // Background opens on Landscapes; Upload, beside the tabs, lights while the player's own picture is in use.
  const [shownTab, setTab] = useState<BackgroundTab>('landscapes');
  const tabs: { id: BackgroundTab; label: string }[] = [
    { id: 'landscapes', label: t('image.tabLandscapes') },
    { id: 'shots', label: t('image.tabShots') },
  ];
  const panelId = useId();
  const shownShots = allScreenshots ? (screenshots.data ?? []) : (screenshots.data ?? []).slice(0, FIRST_SCREENSHOTS);

  /** Clear what the last pick left behind, so only this pick's error can show. */
  const fresh = () => {
    setPickError(null);
    setArt.reset();
    setIcon.reset();
    setView.reset();
  };
  const chooseBackground = (choice: ArtChoice) => {
    if (busy) return;
    fresh();
    setArt.mutate(
      { id: instance.id, choice },
      {
        onSuccess: () => {
          setShot(choice.kind === 'screenshot' ? choice.name : null);
          // A new background starts from the default view.
          setRestart(null);
        },
      },
    );
  };
  const chooseIcon = (choice: IconChoice) => {
    if (busy) return;
    fresh();
    setIcon.mutate({ id: instance.id, choice });
  };
  const saveView = (view: BannerView) => {
    fresh();
    setView.mutate({ id: instance.id, view }, { onError: restartView });
  };
  const upload = async (apply: (path: string) => void) => {
    if (busy) return;
    fresh();
    try {
      const chosen = await open({
        multiple: false,
        directory: false,
        title: t('image.pickTitle'),
        filters: [{ name: t('image.fileFilter'), extensions: IMAGE_EXTENSIONS }],
      });
      if (typeof chosen === 'string') apply(chosen);
    } catch (error: unknown) {
      setPickError(error);
    }
  };
  const error = pickError ?? (setIcon.isError ? setIcon.error : setArt.isError ? setArt.error : setView.isError ? setView.error : null);
  // Only a picture is cropped by the marquee; a landscape is drawn to fit.
  const adjustable = bannerPicture !== null && (background === 'upload' || background === 'pack');

  if (step === 'adjust' && adjustable) {
    return (
      <Dialog
        open
        onClose={onClose}
        title={t('image.adjustTitle')}
        description={t('image.adjustHint')}
        back={{ label: t('common.back'), onBack: () => setStep('choose') }}
      >
        <div className="mt-4 grid gap-4">
          <BannerAdjust
            key={restart?.n ?? 0}
            instance={restart?.instance ?? instance}
            picture={bannerPicture}
            disabled={pictureBusy}
            onPreview={(view) => previewView(instance.id, view)}
            onSave={saveView}
            onDone={onClose}
          />
          {error !== null && <ErrorCard error={error} />}
        </div>
      </Dialog>
    );
  }

  return (
    <Dialog open onClose={onClose} title={t('image.title')} actions={
        <>
          {/* Adjust leads the foot (only a picture is cropped by the marquee); Done ends it. */}
          {adjustable && (
            <Button icon={<Crop />} className="mr-auto" onClick={() => setStep('adjust')}>
              {t('image.adjust')}
            </Button>
          )}
          <Button onClick={onClose}>{t('image.done')}</Button>
        </>
      }>
      <div className="grid gap-5">
        <p>{t('image.body')}</p>
        <div className="art-stack">
          <ChoiceGroup label={t('image.iconSection')} busy={busy}>
            <div className="art-keys art-keys-row">
              <PictureKey
                label={t('image.generatedIcon')}
                hint={t('image.generatedIconHint')}
                glyph={<InstanceIcon name={instance.name} seed={artSeed(instance)} size={SLOT} />}
                fill
                picture={null}
                current={icon === 'generated'}
                tabbable={icon === 'generated'}
                busy={busy}
                onChoose={() => chooseIcon({ kind: 'generated' })}
              />
              {pack && (
                <PictureKey
                  label={t('image.packIcon')}
                  glyph={<Package />}
                  picture={iconPicture}
                  current={icon === 'pack'}
                  tabbable={icon === 'pack'}
                  busy={busy}
                  onChoose={() => chooseIcon({ kind: 'pack' })}
                />
              )}
              <PictureKey
                label={t('image.uploadIcon')}
                hint={t('image.uploadHintIcon')}
                glyph={<Upload />}
                picture={iconPicture}
                current={icon === 'upload'}
                tabbable={icon === 'upload'}
                busy={busy}
                onChoose={() => void upload((path) => chooseIcon({ kind: 'upload', path }))}
              />
            </div>
          </ChoiceGroup>
          <div className="art-rule" aria-hidden="true" />
          <ChoiceGroup
            label={t('image.backgroundSection')}
            busy={busy}
            between={(labelId) => (
              <div className="art-tabs-row">
                <BackgroundTabs tabs={tabs} shown={shownTab} labelId={labelId} panelId={panelId} onShow={setTab} />
                <div className="art-tabs-actions">
                  {pack && (
                    <PictureKey
                      action
                      label={t('image.pack')}
                      hint={t('image.packHint')}
                      glyph={<Package />}
                      picture={bannerPicture}
                      current={background === 'pack'}
                      tabbable
                      busy={busy}
                      onChoose={() => chooseBackground({ kind: 'pack' })}
                    />
                  )}
                  <PictureKey
                    action
                    label={t('image.upload')}
                    hint={t('image.uploadHintBackground')}
                    glyph={<Upload />}
                    picture={bannerPicture}
                    current={uploadLit}
                    tabbable
                    busy={busy}
                    onChoose={() => void upload((path) => chooseBackground({ kind: 'upload', path }))}
                  />
                </div>
              </div>
            )}
          >
            <div role="tabpanel" id={panelId} aria-labelledby={`${panelId}-${shownTab}`} className="art-panel">
              {shownTab === 'landscapes' && (
                <div className="art-tiles">
                  {presetSeeds(instance).map((seed, index) => {
                    const lit = typeof background === 'object' && background.seed === seed;
                    return (
                      <button
                        key={seed}
                        type="button"
                        role="radio"
                        className="art-tile"
                        aria-label={t('image.landscape', { n: index + 1 })}
                        aria-checked={lit}
                        aria-disabled={busy || undefined}
                        // Nothing lit here (a pinned seed that is none of the six, or another tab's choice): the first keeps the tab stop.
                        tabIndex={lit || (index === 0 && !landscapeLit) ? 0 : -1}
                        onClick={() => chooseBackground({ kind: 'generated', seed })}
                      >
                        <Marquee seed={seed} attract={false} className="absolute inset-0" />
                      </button>
                    );
                  })}
                </div>
              )}
              {shownTab === 'shots' &&
                screenshots.data &&
                (screenshots.data.length === 0 ? (
                  <p className="text-sm text-ink-dim">{t('image.noScreenshots')}</p>
                ) : (
                  <>
                    <div className="art-tiles">
                      {shownShots.map((name, index) => (
                        <ScreenshotTile
                          key={name}
                          id={instance.id}
                          name={name}
                          current={shotLit && shot === name}
                          // Nothing lit here: the first keeps the tab stop.
                          tabbable={shotLit ? shot === name : index === 0}
                          busy={busy}
                          onChoose={() => chooseBackground({ kind: 'screenshot', name })}
                        />
                      ))}
                    </div>
                    {!allScreenshots && screenshots.data.length > FIRST_SCREENSHOTS && (
                      <Button variant="ghost" className="justify-self-start" onClick={() => setAllScreenshots(true)}>
                        {t('image.moreScreenshots', {
                          count: screenshots.data.length,
                        })}
                      </Button>
                    )}
                  </>
                ))}
            </div>
          </ChoiceGroup>
        </div>
        {error !== null && <ErrorCard error={error} />}
      </div>
    </Dialog>
  );
}
