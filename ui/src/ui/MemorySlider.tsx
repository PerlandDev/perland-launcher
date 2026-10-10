import { useId } from 'react';
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
  const max = Math.max(1024, Math.min(65536, Math.floor(systemMb / 512) * 512));
  const high = valueMb > systemMb / 2;
  return (
    <div className="grid gap-2">
      <div className="flex items-baseline justify-between">
        <label htmlFor={id} className="text-sm font-semibold">
          {label}
        </label>
        <span className="tabular font-display text-xl">{t('instanceSettings.memoryValue', { gb: formatGb(valueMb, i18n.language) })}</span>
      </div>
      <input
        id={id}
        type="range"
        min={512}
        max={max}
        step={512}
        value={Math.min(valueMb, max)}
        onChange={(e) => onChange(Number(e.target.value))}
        className="accent-[var(--color-amber)]"
      />
      <p className={high ? 'text-sm text-coral' : 'text-sm text-ink-dim'}>
        {high ? t('instanceSettings.memoryHigh') : t('instanceSettings.memoryHint', { total: formatGb(systemMb, i18n.language) })}
      </p>
    </div>
  );
}
