// A wardrobe skin's kept versions, newest first, in the texture column. Picking one previews it on the stage.
import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { commands } from '../../ipc/commands';
import type { SkinModel, SkinVersion } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { Close, Trash } from '../../ui/Icon';
import { whenLabel } from './when';

/** Where each part's front face sits in the texture ([x, y, w, h]) and on the 16 × 32 figure ([x, y]), base then outer. */
function frontParts(model: SkinModel): [number, number, number, number, number, number][] {
  const arm = model === 'slim' ? 3 : 4;
  return [
    [8, 8, 8, 8, 4, 0],
    [20, 20, 8, 12, 4, 8],
    [44, 20, arm, 12, 4 - arm, 8],
    [36, 52, arm, 12, 12, 8],
    [4, 20, 4, 12, 4, 20],
    [20, 52, 4, 12, 8, 20],
    [40, 8, 8, 8, 4, 0],
    [20, 36, 8, 12, 4, 8],
    [44, 36, arm, 12, 4 - arm, 8],
    [52, 52, arm, 12, 12, 8],
    [4, 36, 4, 12, 4, 20],
    [4, 52, 4, 12, 8, 20],
  ];
}

/** A version as a tiny front view of the whole figure, so a changed shirt shows as well as a changed face. */
function VersionFigure({ skin, version, model }: { skin: string; version: string; model: SkinModel }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const texture = useQuery({ queryKey: ['skinVersionTexture', skin, version], queryFn: () => commands.skinVersionTexture(skin, version), staleTime: Infinity });
  useEffect(() => {
    const ctx = canvas.current?.getContext('2d');
    if (!ctx || !texture.data) return;
    const image = new Image();
    image.onload = () => {
      ctx.imageSmoothingEnabled = false;
      ctx.clearRect(0, 0, 16, 32);
      for (const [sx, sy, w, h, dx, dy] of frontParts(model)) ctx.drawImage(image, sx, sy, w, h, dx, dy, w, h);
    };
    image.src = texture.data;
  }, [texture.data, model]);
  return <canvas ref={canvas} width={16} height={32} className="history-figure" aria-hidden />;
}

export function HistoryPanel({
  skin,
  model,
  versions,
  picked,
  onPick,
  onClear,
  onClose,
}: {
  skin: string;
  model: SkinModel;
  versions: SkinVersion[] | undefined;
  picked: string | null;
  onPick: (version: SkinVersion) => void;
  /** Asks to forget every version (the studio confirms first). */
  onClear: () => void;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  return (
    <aside className="skins-side history" aria-labelledby="history-title">
      <div className="flex items-center justify-between gap-2">
        <h2 id="history-title" className="font-bold">
          {t('skins.history.title')}
        </h2>
        <span className="ml-auto text-sm text-ink-dim tabular">{versions?.length ?? ''}</span>
        <Button variant="ghost" size="icon" aria-label={t('skins.history.close')} title={t('skins.history.close')} icon={<Close />} onClick={onClose} />
      </div>
      <p className="text-sm text-ink-dim text-pretty">{t('skins.history.about')}</p>
      {versions && versions.length === 0 ? (
        <p className="history-empty text-sm text-ink-dim">{t('skins.history.empty')}</p>
      ) : (
        <ul className="history-list" aria-label={t('skins.history.list')}>
          {(versions ?? []).map((v) => {
            const isPicked = v.id === picked;
            return (
              <li key={v.id} className="wardrobe-item" data-selected={isPicked || undefined}>
                <button type="button" className="wardrobe-pick" aria-current={isPicked || undefined} onClick={() => onPick(v)}>
                  <VersionFigure skin={skin} version={v.id} model={v.model ?? model} />
                  <span className="grid min-w-0 flex-1 text-left">
                    <span className="truncate font-semibold tabular">{whenLabel(v.taken, i18n.language, t)}</span>
                    <span className="flex items-center gap-1.5 text-xs text-ink-dim">
                      {v.worn && <span className="lamp" aria-hidden />}
                      {t(`skins.history.reason.${v.reason}`)}
                      {v.worn && v.reason !== 'worn' && <> · {t('skins.history.reason.worn')}</>}
                    </span>
                  </span>
                  {v.current && <span className="history-now">{t('skins.history.now')}</span>}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {/* At the foot, away from the versions: forgetting them all is the one thing here that can't be undone. */}
      {versions && versions.length > 0 && (
        <Button variant="ghost" className="history-clear" icon={<Trash />} onClick={onClear}>
          {t('skins.history.clear')}
        </Button>
      )}
    </aside>
  );
}
