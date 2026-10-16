import { open } from '@tauri-apps/plugin-dialog';
import { useId, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Marquee } from '../../art/Marquee';
import { artSeed, presetSeeds } from '../../art/seed';
import { pixelUpscale } from '../../art/upscale';
import type { ArtChoice, Instance } from '../../ipc/schema';
import { useInstanceArt, useSetInstanceArt } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Package, Upload } from '../../ui/Icon';
import { isPack } from './labels';

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp'];
/** The icon slot on the Pack art and Upload keys, in px. */
const SLOT = 32;

/** Which choice is in use now (ruling R4): an uploaded picture, the pack's own art, or a landscape's seed
 *  (which lights no tile when it isn't one of the six). */
function currentChoice(instance: Instance): 'upload' | 'pack' | { seed: string } {
  if (instance.art === 'custom') return 'upload';
  if (instance.art === 'auto' && isPack(instance)) return 'pack';
  return { seed: artSeed(instance) };
}

/** A picture-choice key: a raised key with its icon slot, pressed into its slot with the lime lamp while in use. */
function PictureKey({
  label,
  hint,
  glyph,
  picture,
  current,
  busy,
  onChoose,
}: {
  label: string;
  /** A second, dim line under the label (what an upload may be). It describes the key; the label alone names it. */
  hint?: string;
  glyph: ReactNode;
  /** The picture in use (a data: URL) when this is the current choice; otherwise the glyph shows. */
  picture: string | null;
  current: boolean;
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
      className="art-key"
      aria-pressed={current}
      aria-disabled={busy || undefined}
      aria-labelledby={`${id}-label`}
      aria-describedby={hint ? `${id}-hint` : undefined}
      onClick={onChoose}
    >
      <span className="art-key-slot" aria-hidden>
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

/** "Change image…": six generated landscapes, the pack's own art (packs only) and an uploaded picture.
 *  A choice applies at once; the lit tile follows it, and the rail and marquee behind the dialog change with it. */
export function ImageDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const { t } = useTranslation();
  const set = useSetInstanceArt({ inline: true });
  const art = useInstanceArt(instance.id);
  const [pickError, setPickError] = useState<unknown>(null);
  const pack = isPack(instance);
  const current = currentChoice(instance);
  const busy = set.isPending;
  // An upload is the background, which comes back apart from the icon.
  const picture = (current === 'upload' ? art.data?.banner : art.data?.icon) ?? null;

  const choose = (choice: ArtChoice) => {
    if (busy) return;
    setPickError(null);
    set.mutate({ id: instance.id, choice });
  };
  const upload = async () => {
    if (busy) return;
    setPickError(null);
    set.reset();
    try {
      const chosen = await open({
        multiple: false,
        directory: false,
        title: t('image.pickTitle'),
        filters: [{ name: t('image.fileFilter'), extensions: IMAGE_EXTENSIONS }],
      });
      if (typeof chosen === 'string') choose({ kind: 'upload', path: chosen });
    } catch (error: unknown) {
      setPickError(error);
    }
  };
  const error = pickError ?? (set.isError ? set.error : null);

  return (
    <Dialog open onClose={onClose} title={t('image.title')} actions={<Button onClick={onClose}>{t('image.done')}</Button>}>
      <div className="grid gap-5">
        <p>{t(pack ? 'image.bodyPack' : 'image.body')}</p>
        <div className="art-tiles" aria-busy={busy || undefined}>
          {presetSeeds(instance).map((seed, index) => (
            <button
              key={seed}
              type="button"
              className="art-tile"
              aria-label={t('image.landscape', { n: index + 1 })}
              aria-pressed={typeof current === 'object' && current.seed === seed}
              aria-disabled={busy || undefined}
              onClick={() => choose({ kind: 'generated', seed })}
            >
              <Marquee seed={seed} attract={false} className="absolute inset-0" />
            </button>
          ))}
        </div>
        <div className="art-keys">
          {pack && (
            <PictureKey
              label={t('image.pack')}
              glyph={<Package />}
              picture={picture}
              current={current === 'pack'}
              busy={busy}
              onChoose={() => choose({ kind: 'pack' })}
            />
          )}
          <PictureKey
            label={t('image.upload')}
            hint={t('image.uploadHint')}
            glyph={<Upload />}
            picture={picture}
            current={current === 'upload'}
            busy={busy}
            onChoose={() => void upload()}
          />
        </div>
        {error !== null && <ErrorCard error={error} />}
      </div>
    </Dialog>
  );
}
