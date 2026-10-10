import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { taskLabel } from '../ipc/errors';
import type { RunStateKind } from '../ipc/schema';
import type { ProgressView } from '../state/runStore';
import { formatBytes, formatCount } from './format';
import { Meter } from './Meter';

export interface PlayButtonProps {
  state: RunStateKind;
  progress: ProgressView | null;
  disabled?: boolean;
  disabledReason?: string;
  onPlay: () => void;
  onStop: () => void;
}

export function PlayButton({ state, progress, disabled, disabledReason, onPlay, onStop }: PlayButtonProps) {
  const { t, i18n } = useTranslation();
  const reasonId = useId();
  const lang = i18n.language;

  if (state === 'queued' || state === 'preparing') {
    const label =
      state === 'queued' ? t('play.queued') : progress ? taskLabel(i18n, progress.task) : t('play.preparing');
    const fraction = progress && progress.total > 0 ? progress.done / progress.total : 0;
    return (
      <button type="button" className="play" data-state={state} onClick={onStop} aria-label={t('play.cancelLabel', { label })}>
        <span className="flex items-baseline justify-between gap-3">
          <span className="truncate">{label}</span>
          <span className="text-ink-dim">{t('play.cancel')}</span>
        </span>
        <Meter value={fraction} label={label} decorative />
        {progress && progress.total > 0 && (
          <span className="tabular text-sm font-normal text-ink-dim">
            {t('play.files', { done: formatCount(progress.done, lang), total: formatCount(progress.total, lang) })}
            {progress.bytes > 0 && ` · ${formatBytes(progress.bytes, lang)}`}
          </span>
        )}
      </button>
    );
  }

  if (state === 'running' || state === 'stopping') {
    return (
      <button type="button" className="play" data-state="running" onClick={onStop} disabled={state === 'stopping'}>
        {state === 'stopping' ? t('play.stopping') : t('play.stop')}
      </button>
    );
  }

  return (
    <>
      <button
        type="button"
        className="play"
        data-state="idle"
        onClick={onPlay}
        disabled={disabled}
        aria-describedby={disabled && disabledReason ? reasonId : undefined}
      >
        {t('play.play')}
      </button>
      {disabled && disabledReason && (
        <span id={reasonId} className="sr-only">
          {disabledReason}
        </span>
      )}
    </>
  );
}
