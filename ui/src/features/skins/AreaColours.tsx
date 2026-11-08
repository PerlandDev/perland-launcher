// The colour areas of the piece picked on the Create tab: per area its quick colours, the system picker and a hex field.
import { useTranslation } from 'react-i18next';
import type { CatalogPiece, WornPiece } from '../../ipc/schema';
import { ColourField } from './ColourPanel';
import type { Lang } from './catalogSearch';
import { toHex } from './paint';

const SKIN_TONES = ['#f6d7c3', '#eec1a0', '#d9a27c', '#c68863', '#a86b4b', '#8d5524', '#6b3e26', '#3f2a1f'];
const CLOTH = ['#c0392b', '#2e86de', '#6a994e', '#f2c14e', '#7d3cff', '#ece6da', '#2b2b2b', '#8b5a2b'];

/** The quick colours of an area: its own default first, then skin tones for skin, cloth colours for the rest. */
function quickColours(areaId: string, fallback: string): string[] {
  const all = [fallback.toLowerCase(), ...(areaId === 'skin' ? SKIN_TONES : CLOTH)];
  return [...new Set(all)].slice(0, 7);
}

export function AreaColours({
  piece,
  worn,
  lang,
  onColour,
}: {
  piece: CatalogPiece;
  worn: WornPiece;
  lang: Lang;
  /** `live`: a colour still being dragged or typed. */
  onColour: (areaId: string, hex: string, live: boolean) => void;
}) {
  const { t } = useTranslation();
  const name = piece.name[lang];
  return (
    <section className="area-colours" aria-labelledby={`area-colours-${piece.id}`}>
      <h3 id={`area-colours-${piece.id}`} className="create-heading">
        {t('skins.create.colours', { name })}
      </h3>
      {piece.areas.length === 0 && <p className="text-sm text-ink-dim">{t('skins.create.noColours', { name })}</p>}
      {piece.areas.map((area) => {
        const value = (worn.colours[area.id] ?? area.default).toLowerCase();
        const areaName = area.name[lang];
        return (
          <div key={area.id} className="area-row">
            <span className="area-name">{areaName}</span>
            <div className="grid min-w-0 gap-2">
              <div className="area-swatches" role="group" aria-label={areaName}>
                {quickColours(area.id, area.default).map((c) => (
                  <button
                    key={c}
                    type="button"
                    className="colour-swatch"
                    style={{ background: c }}
                    aria-label={t('skins.create.quickColour', { area: areaName, value: c })}
                    aria-pressed={c === value}
                    title={c}
                    onClick={() => onColour(area.id, c, false)}
                  />
                ))}
              </div>
              <ColourField color={value} label={areaName} onPreview={(c) => onColour(area.id, toHex(c), true)} onColor={(c) => onColour(area.id, toHex(c), true)} />
            </div>
          </div>
        );
      })}
    </section>
  );
}
