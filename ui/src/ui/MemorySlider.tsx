import { type CSSProperties, useId } from 'react';
import { useTranslation } from 'react-i18next';
import { formatGb } from './format';

export function MemorySlider({
  label,
  valueMb,
  onChange,
  systemMb,
}: {
  label: string;
  valueMb: number;
  onChange: (mb: number) => void;
  systemMb: number;
}) {
  const { t, i18n } = useTranslation();
  const id = useId();
  const hintId = `${id}-hint`;
  // The backend reports 0 when it can't read the computer's memory: then there is no honest limit or warning.
  const known = systemMb > 0;
  // Never below the current value, so an existing setting keeps its thumb consistent with its label.
  const max = Math.max(1024, known ? Math.min(65536, Math.floor(systemMb / 512) * 512) : 65536, valueMb);
  const high = known && valueMb > systemMb / 2;
  const shown = t('instanceSettings.memoryValue', { gb: formatGb(valueMb, i18n.language) });
  return (
    <div className="grid gap-2">
      <div className="flex items-baseline justify-between">
        <label htmlFor={id} className="text-sm font-semibold">
          {label}
        </label>
        <span className="tabular font-display text-xl">{shown}</span>
      </div>
      <input
        id={id}
        type="range"
        min={512}
        max={max}
        step={512}
        value={valueMb}
        aria-valuetext={shown}
        aria-describedby={hintId}
        onChange={(e) => onChange(Number(e.target.value))}
        className="range"
        style={{ '--fill': `${((Math.min(valueMb, max) - 512) / Math.max(1, max - 512)) * 100}%` } as CSSProperties}
      />
      <p id={hintId} className={high ? 'text-sm text-blue-ink' : 'text-sm text-ink-dim'}>
        {high
          ? t('instanceSettings.memoryHigh')
          : known
            ? t('instanceSettings.memoryHint', { total: formatGb(systemMb, i18n.language) })
            : t('instanceSettings.memoryHintUnknown')}
      </p>
    </div>
  );
}
