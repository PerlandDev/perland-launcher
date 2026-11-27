// The colour areas of the piece picked on the Create tab: per area the system picker and a hex field, and under them
// the quick colours that suit the area (skin tones, eye colours, hair colours, or the cloth palette).
import type { KeyboardEvent, Ref } from 'react';
import { useTranslation } from 'react-i18next';
import type { CatalogPiece, WornPiece } from '../../ipc/schema';
import { ColourField } from './ColourPanel';
import type { Lang } from './catalogSearch';
import { toHex } from './paint';
import { paletteFor } from './palettes';

/** Arrow keys, Home and End walk the swatches of one area, so the whole row is a single stop for Tab. */
function walk(e: KeyboardEvent<HTMLDivElement>) {
  const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
  if (step === undefined && e.key !== 'Home' && e.key !== 'End') return;
  const all = [...e.currentTarget.querySelectorAll<HTMLButtonElement>('button')];
  const at = all.indexOf(document.activeElement as HTMLButtonElement);
  if (at < 0) return;
  e.preventDefault();
  const to = e.key === 'Home' ? 0 : e.key === 'End' ? all.length - 1 : (at + step! + all.length) % all.length;
  all[to]!.focus();
}

export function AreaColours({
  piece,
  worn,
  lang,
  onColour,
  ref,
}: {
  /** The section, for what brings it into view. */
  ref?: Ref<HTMLElement>;
  piece: CatalogPiece;
  worn: WornPiece;
  lang: Lang;
  /** `live`: a colour still being dragged or typed. */
  onColour: (areaId: string, hex: string, live: boolean) => void;
}) {
  const { t } = useTranslation();
  const name = piece.name[lang];
  return (
    <section ref={ref} className="area-colours" aria-labelledby={`area-colours-${piece.id}`}>
      <h3 id={`area-colours-${piece.id}`} className="create-heading">
        {t('skins.create.colours', { name })}
      </h3>
      {piece.areas.length === 0 && <p className="text-sm text-ink-dim">{t('skins.create.noColours', { name })}</p>}
      {piece.areas.map((area) => {
        const value = (worn.colours[area.id] ?? area.default).toLowerCase();
        const areaName = area.name[lang];
        const palette = paletteFor(piece.slot, area.id);
        // The swatch in use takes Tab; with none in use, the first.
        const stop = Math.max(0, palette.indexOf(value));
        return (
          <div key={area.id} className="area-row">
            <span className="area-name">{areaName}</span>
            <div className="grid min-w-0 gap-2">
              <ColourField color={value} label={areaName} onPreview={(c) => onColour(area.id, toHex(c), true)} onColor={(c) => onColour(area.id, toHex(c), true)} />
              <div className="area-swatches" role="toolbar" aria-label={areaName} onKeyDown={walk}>
                {palette.map((c, i) => (
                  <button
                    key={c}
                    type="button"
                    className="colour-swatch"
                    style={{ background: c }}
                    aria-label={t('skins.create.quickColour', { area: areaName, value: c })}
                    aria-pressed={c === value}
                    tabIndex={i === stop ? 0 : -1}
                    title={c}
                    onClick={() => onColour(area.id, c, false)}
                  />
                ))}
              </div>
            </div>
          </div>
        );
      })}
    </section>
  );
}
