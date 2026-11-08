import { useEffect, useId, useRef, useState, type CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';
import { fromHex, toHex, type Rgba } from './paint';
import { PRESETS } from './tools';

/** How long the picker or the hex field must rest on a colour before it joins the recent row. */
export const SETTLE_MS = 500;

/**
 * The colour well (the system picker) beside a hex field. `onPreview` hears a colour the player is still moving
 * through (dragging in the picker, typing a hex); `onColor` hears it once it rests for `SETTLE_MS` or the field is
 * left. A new `color` from outside (a swatch) drops a colour still settling. Shared by Paint's colours and Create's
 * colour areas.
 */
export function ColourField({
  color,
  chipAlpha = 100,
  label,
  onColor,
  onPreview,
}: {
  /** `#rrggbb`. */
  color: string;
  /** The chip shows the colour at this opacity (percent) over the checker. */
  chipAlpha?: number;
  /** Names the field for a screen reader; the picker and hex field add their own words to it. */
  label?: string;
  onColor: (c: Rgba) => void;
  onPreview: (c: Rgba) => void;
}) {
  const { t } = useTranslation();
  const hex = color.toLowerCase();
  const settle = useRef<{ timer: ReturnType<typeof setTimeout>; color: Rgba } | null>(null);
  const commit = () => {
    const pending = settle.current;
    if (!pending) return;
    clearTimeout(pending.timer);
    settle.current = null;
    onColor(pending.color);
  };
  const preview = (c: Rgba) => {
    onPreview(c);
    if (settle.current) clearTimeout(settle.current.timer);
    settle.current = { color: c, timer: setTimeout(commit, SETTLE_MS) };
  };
  // A colour chosen from outside (a swatch) overrides one still settling: that one is never committed.
  useEffect(() => {
    if (settle.current && toHex(settle.current.color) !== hex) {
      clearTimeout(settle.current.timer);
      settle.current = null;
    }
  }, [hex]);
  // A colour still settling when the field goes away is dropped, not committed late.
  useEffect(() => () => clearTimeout(settle.current?.timer), []);
  // What the player is typing, for as long as the colour is still the one they typed it against.
  const [draft, setDraft] = useState<{ text: string; for: string } | null>(null);
  const text = draft && draft.for === hex ? draft.text : hex;
  const hexId = useId();
  const more = label ? t('skins.moreColoursOf', { name: label }) : t('skins.moreColours');
  const hexLabel = label ? t('skins.hexOf', { name: label }) : t('skins.hex');
  return (
    <div className="flex items-center gap-3">
      <label className="colour-well" title={t('skins.moreColours')}>
        <span className="colour-chip" style={{ '--chip': hex, '--chip-alpha': `${chipAlpha}%` } as CSSProperties} aria-hidden />
        <input type="color" className="sr-only" value={hex} aria-label={more} onChange={(e) => preview(fromHex(e.target.value)!)} onBlur={commit} />
      </label>
      <div className="grid min-w-0 flex-1 gap-1">
        <label htmlFor={hexId} className="sr-only">
          {hexLabel}
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
            if (parsed) preview(parsed);
          }}
          onBlur={() => {
            setDraft(null);
            commit();
          }}
        />
      </div>
    </div>
  );
}

/**
 * The colour and how strongly it goes on. `onColor` chooses a colour for good (it joins the recent row); `onPreview`
 * shows one the player is still moving through (dragging in the picker, typing a hex), which only joins the recent row
 * once it settles.
 */
export function ColourPanel({
  color,
  opacity,
  recent,
  onColor,
  onPreview,
  onOpacity,
}: {
  color: Rgba;
  /** 1–100 (percent). */
  opacity: number;
  recent: string[];
  onColor: (c: Rgba) => void;
  onPreview: (c: Rgba) => void;
  onOpacity: (percent: number) => void;
}) {
  const { t } = useTranslation();
  const hex = toHex(color);
  const opacityId = useId();
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
      <ColourField color={hex} chipAlpha={opacity} onColor={onColor} onPreview={onPreview} />
      <div className="opacity-row">
        <label htmlFor={opacityId} className="text-sm font-semibold">
          {t('skins.opacity')}
        </label>
        <input
          id={opacityId}
          type="range"
          min={1}
          max={100}
          step={1}
          value={opacity}
          aria-valuetext={t('skins.opacityValue', { value: opacity })}
          onChange={(e) => onOpacity(Number(e.target.value))}
          className="range"
          style={{ '--fill': `${((opacity - 1) / 99) * 100}%` } as CSSProperties}
        />
        <span className="opacity-value tabular font-display" aria-hidden>
          {t('skins.opacityValue', { value: opacity })}
        </span>
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
