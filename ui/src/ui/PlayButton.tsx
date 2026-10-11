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

  const waiting = state === 'checking' || state === 'asking' || state === 'queued' || state === 'syncing' || state === 'preparing';
  if (waiting) {
    const label =
      state === 'checking'
        ? t('play.checking')
        : state === 'asking'
          ? t('play.asking')
          : state === 'queued'
            ? t('play.queued')
            : progress
              ? taskLabel(i18n, progress.task)
              : state === 'syncing'
                ? t('play.syncing')
                : t('play.preparing');
    const fraction = progress && progress.total > 0 ? progress.done / progress.total : 0;
    // Syncing and preparing fill the meter; the other waits show it empty.
    const look = state === 'syncing' || state === 'preparing' ? 'preparing' : 'queued';
    return (
      <button type="button" className="play" data-state={look} onClick={onStop} aria-label={t('play.cancelLabel', { label })}>
        {/* The phase gets the face's full width (Spanish phases run long); Cancel rides the readout row,
            which every wait state keeps, so the face holds one height from checking to preparing. */}
        <span className="truncate">{label}</span>
        <Meter value={fraction} label={label} decorative scan={state === 'checking'} />
        <span className="play-sub flex items-baseline gap-2 text-sm">
          {/* The MB figure is the honest part of the readout: only the file counts may truncate.
              Four-digit totals (a first install's assets) drop to "done/total"; the drawer keeps the sentence. */}
          <span className="flex min-w-0 flex-1 items-baseline font-normal">
            {progress && progress.total > 0 && (
              <>
                <span className="tabular truncate">
                  {t(progress.total >= 1000 ? 'play.filesShort' : 'play.files', {
                    done: formatCount(progress.done, lang),
                    total: formatCount(progress.total, lang),
                  })}
                </span>
                {progress.bytes > 0 && <span className="tabular flex-none whitespace-pre">{` · ${formatBytes(progress.bytes, lang)}`}</span>}
              </>
            )}
          </span>
          <span className="flex-none">{t('play.cancel')}</span>
        </span>
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
