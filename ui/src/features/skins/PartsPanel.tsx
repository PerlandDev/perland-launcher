import { useTranslation } from 'react-i18next';
import { Eye, EyeOff } from 'pixelarticons/react';
import { LAYERS, PARTS, type Layer } from './layout';

/** Which parts show on the figure, per layer. Each column head turns its whole layer on or off: the Outer head is the
 *  outer layer's one switch. */
export function PartsPanel({ hidden, onToggle, onLayer }: { hidden: Set<string>; onToggle: (key: string) => void; onLayer: (layer: Layer, show: boolean) => void }) {
  const { t } = useTranslation();
  const shown = (layer: Layer) => PARTS.some((p) => !hidden.has(`${p}:${layer}`));
  return (
    <section className="parts-panel" aria-labelledby="skins-parts-title">
      <div className="parts-grid">
        <h2 id="skins-parts-title" className="parts-title">
          {t('skins.parts')}
        </h2>
        {LAYERS.map((layer) => {
          const on = shown(layer);
          return (
            <button
              key={layer}
              type="button"
              className="parts-head"
              aria-pressed={on}
              aria-label={t('skins.layerEverywhere', { layer: t(`skins.layer.${layer}`) })}
              title={t('skins.layerEverywhere', { layer: t(`skins.layer.${layer}`) })}
              onClick={() => onLayer(layer, !on)}
            >
              {on ? <Eye aria-hidden /> : <EyeOff aria-hidden />}
              <span>{t(`skins.layer.${layer}`)}</span>
            </button>
          );
        })}
        {PARTS.map((part) => (
          <div key={part} className="contents">
            <span className="parts-name">{t(`skins.part.${part}`)}</span>
            {LAYERS.map((layer) => (
              <input
                key={layer}
                type="checkbox"
                className="check"
                aria-label={`${t(`skins.part.${part}`)} · ${t(`skins.layer.${layer}`)}`}
                checked={!hidden.has(`${part}:${layer}`)}
                onChange={() => onToggle(`${part}:${layer}`)}
              />
            ))}
          </div>
        ))}
      </div>
    </section>
  );
}
