import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { fromHex, toHex, type Rgba } from './paint';
import { PRESETS } from './tools';


export function ColourPanel({ color, recent, onColor }: { color: Rgba; recent: string[]; onColor: (c: Rgba) => void }) {
  const { t } = useTranslation();
  const hex = toHex(color);
  // What the player is typing, for as long as the colour is still the one they typed it against.
  const [draft, setDraft] = useState<{ text: string; for: string } | null>(null);
  const text = draft && draft.for === hex ? draft.text : hex;
  const hexId = useId();
  const swatch = (value: string, label: string) => (
    <button
      key={label}
      type="button"
      className="colour-swatch"
      style={{ background: value }}
      aria-label={label}
      aria-pressed={value === hex}
      title={value}
      onClick={() => onColor(fromHex(value)!)}
    />
  );
  return (
    <section className="grid gap-2" aria-label={t('skins.colours')}>
      <div className="flex items-center gap-3">
        <label className="colour-well" title={t('skins.moreColours')}>
          <span className="colour-chip" style={{ background: hex }} aria-hidden />
          <input
            type="color"
            className="sr-only"
            value={hex}
            aria-label={t('skins.moreColours')}
            onChange={(e) => onColor(fromHex(e.target.value)!)}
          />
        </label>
        <div className="grid min-w-0 flex-1 gap-1">
          <label htmlFor={hexId} className="sr-only">
            {t('skins.hex')}
          </label>
          <input
            id={hexId}
            className="field-input tabular font-mono"
            value={text}
            spellCheck={false}
            maxLength={7}
            onChange={(e) => {
              const value = e.target.value;
              const parsed = value.replace('#', '').length === 6 ? fromHex(value) : null;
              setDraft({ text: value, for: parsed ? toHex(parsed) : hex });
              if (parsed) onColor(parsed);
            }}
            onBlur={() => setDraft(null)}
          />
        </div>
      </div>
      {recent.length > 0 && (
        <div className="swatches" role="group" aria-label={t('skins.recent')} title={t('skins.recent')}>
          {recent.map((value) => swatch(value, t('skins.recentColour', { value })))}
        </div>
      )}
      <div className="swatches swatches-presets">{PRESETS.map((value) => swatch(value, t('skins.presetColour', { value })))}</div>
    </section>
  );
}
