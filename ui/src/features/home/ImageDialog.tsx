import { useQueryClient } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';
import { useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { InstanceIcon } from '../../art/InstanceIcon';
import { Marquee } from '../../art/Marquee';
import { artSeed, presetSeeds } from '../../art/seed';
import { pixelUpscale } from '../../art/upscale';
import type { ArtChoice, Focus, IconChoice, Instance, InstanceListing } from '../../ipc/schema';
import {
  qk,
  useInstanceArt,
  useInstanceScreenshot,
  useInstanceScreenshots,
  usePreviewBannerFocus,
  useSetBannerFocus,
  useSetInstanceArt,
  useSetInstanceIcon,
} from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Package, Upload } from '../../ui/Icon';
import { FocusPicker, type FocusPickerHandle } from './FocusPicker';
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
 *  Enter picks. Moving never picks, because every pick saves at once and an Upload key opens the file picker. */
function ChoiceGroup({ label, busy, children }: { label: string; busy: boolean; children: ReactNode }) {
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
  return (
    <div role="radiogroup" aria-labelledby={id} aria-busy={busy || undefined} className="art-section" onKeyDown={onKeyDown}>
      <h3 id={id} className="art-section-label">
        {label}
      </h3>
      {children}
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
}: {
  label: string;
  /** A second, dim line under the label (what an upload may be). It describes the key; the label alone names it. */
  hint?: string;
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
      role="radio"
      className="art-key"
      aria-checked={current}
      aria-disabled={busy || undefined}
      aria-labelledby={`${id}-label`}
      aria-describedby={hint ? `${id}-hint` : undefined}
      tabIndex={tabbable ? 0 : -1}
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

/** "Change image…": two groups in one dialog. Icon: the initials, the pack's icon (packs only) or an
 *  uploaded picture. Background: six generated landscapes, the pack's art (packs only), an uploaded
 *  picture or one of the instance's screenshots. A choice applies at once; the lit key follows it, and the rail and marquee behind the dialog
 *  change with it. While either saves, both groups wait: two saves at once would race on instance.json. */
export function ImageDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const { t } = useTranslation();
  const setArt = useSetInstanceArt({ inline: true });
  const setIcon = useSetInstanceIcon({ inline: true });
  const setFocus = useSetBannerFocus({ inline: true });
  const previewFocus = usePreviewBannerFocus();
  const art = useInstanceArt(instance.id);
  const screenshots = useInstanceScreenshots(instance.id);
  const [allScreenshots, setAllScreenshots] = useState(false);
  // The screenshot picked here, while it is still the background. The core keeps it as the player's own picture,
  // so the dialog opened again lights Upload for it.
  const [shot, setShot] = useState<string | null>(null);
  const [pickError, setPickError] = useState<unknown>(null);
  const qc = useQueryClient();
  // When a new background lands or a position is refused, Position starts again from what is saved now. The
  // cache has it before this dialog's `instance` does.
  const [restart, setRestart] = useState<{
    n: number;
    focus: Focus | null;
  } | null>(null);
  const restartFocus = () =>
    setRestart((last) => ({
      n: (last?.n ?? 0) + 1,
      focus: qc.getQueryData<InstanceListing>(qk.instances)?.instances.find((i) => i.id === instance.id)?.bannerFocus ?? null,
    }));
  const focusPicker = useRef<FocusPickerHandle>(null);
  const pack = isPack(instance);
  const background = currentBackground(instance);
  const icon = currentIcon(instance, art.data && art.data.icon);
  const pictureBusy = setArt.isPending || setIcon.isPending;
  const busy = pictureBusy || setFocus.isPending;
  // The icon and the background come back apart, each shown on its own current key.
  const iconPicture = art.data?.icon ?? null;
  const bannerPicture = art.data?.banner ?? null;
  const landscapeLit = typeof background === 'object' && presetSeeds(instance).includes(background.seed);
  const shotLit = background === 'upload' && shot !== null && (screenshots.data ?? []).includes(shot);
  const uploadLit = background === 'upload' && !shotLit;
  const shownShots = allScreenshots ? (screenshots.data ?? []) : (screenshots.data ?? []).slice(0, FIRST_SCREENSHOTS);

  /** Clear what the last pick left behind, so only this pick's error can show. */
  const fresh = () => {
    setPickError(null);
    setArt.reset();
    setIcon.reset();
    setFocus.reset();
  };
  const chooseBackground = (choice: ArtChoice) => {
    if (busy) return;
    fresh();
    // A new background starts from the default position, so a position still waiting to be saved goes.
    focusPicker.current?.cancel();
    setArt.mutate(
      { id: instance.id, choice },
      {
        onSuccess: () => {
          setShot(choice.kind === 'screenshot' ? choice.name : null);
          restartFocus();
        },
      },
    );
  };
  const chooseIcon = (choice: IconChoice) => {
    if (busy) return;
    fresh();
    // Saves run one after another, so a position still waiting lands first.
    focusPicker.current?.flush();
    setIcon.mutate({ id: instance.id, choice });
  };
  const saveFocus = (focus: Focus | null) => {
    fresh();
    setFocus.mutate({ id: instance.id, focus }, { onError: restartFocus });
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
  const error = pickError ?? (setIcon.isError ? setIcon.error : setArt.isError ? setArt.error : setFocus.isError ? setFocus.error : null);

  return (
    <Dialog open onClose={onClose} title={t('image.title')} actions={<Button onClick={onClose}>{t('image.done')}</Button>}>
      <div className="grid gap-5">
        <p>{t('image.body')}</p>
        <div className="art-sections">
          <ChoiceGroup label={t('image.iconSection')} busy={busy}>
            <div className="art-keys art-keys-stack">
              <PictureKey
                label={t('image.generatedIcon')}
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
          <div className="art-column">
            <ChoiceGroup label={t('image.backgroundSection')} busy={busy}>
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
                      // A pinned seed that is none of the six lights nothing; the first landscape keeps the tab stop.
                      tabIndex={lit || (index === 0 && typeof background === 'object' && !landscapeLit) ? 0 : -1}
                      onClick={() => chooseBackground({ kind: 'generated', seed })}
                    >
                      <Marquee seed={seed} attract={false} className="absolute inset-0" />
                    </button>
                  );
                })}
              </div>
              <div className="art-keys">
                {pack && (
                  <PictureKey
                    label={t('image.pack')}
                    glyph={<Package />}
                    picture={bannerPicture}
                    current={background === 'pack'}
                    tabbable={background === 'pack'}
                    busy={busy}
                    onChoose={() => chooseBackground({ kind: 'pack' })}
                  />
                )}
                <PictureKey
                  label={t('image.upload')}
                  hint={t('image.uploadHintBackground')}
                  glyph={<Upload />}
                  picture={bannerPicture}
                  current={uploadLit}
                  tabbable={uploadLit}
                  busy={busy}
                  onChoose={() => void upload((path) => chooseBackground({ kind: 'upload', path }))}
                />
              </div>
              {screenshots.data && (
                <div className="grid gap-2">
                  <h4 className="art-section-label">{t('image.screenshots')}</h4>
                  {screenshots.data.length === 0 ? (
                    <p className="text-sm text-ink-dim">{t('image.noScreenshots')}</p>
                  ) : (
                    <>
                      <div className="art-tiles">
                        {shownShots.map((name) => (
                          <ScreenshotTile
                            key={name}
                            id={instance.id}
                            name={name}
                            current={shotLit && shot === name}
                            tabbable={shotLit && shot === name}
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
                  )}
                </div>
              )}
            </ChoiceGroup>
            {/* Only a picture is cropped by the marquee; a landscape is drawn to fit. */}
            {bannerPicture && (background === 'upload' || background === 'pack') && (
              <FocusPicker
                key={`${background}-${restart?.n ?? 0}`}
                ref={focusPicker}
                picture={bannerPicture}
                focus={restart ? restart.focus : instance.bannerFocus}
                disabled={pictureBusy}
                onPreview={(focus) => previewFocus(instance.id, focus)}
                onSave={saveFocus}
              />
            )}
          </div>
        </div>
        {error !== null && <ErrorCard error={error} />}
      </div>
    </Dialog>
  );
}
