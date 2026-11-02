import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { MAX_WINDOW_SIDE, MIN_WINDOW, side, type WindowDraft, type WindowMode } from './gameWindow';

export function GameWindowFields({ draft, onChange }: { draft: WindowDraft; onChange: (draft: WindowDraft) => void }) {
  const { t } = useTranslation();
  const id = useId();
  const noteId = `${id}-note`;
  const widthBad = side(draft.width, MIN_WINDOW.width) === null;
  const heightBad = side(draft.height, MIN_WINDOW.height) === null;
  const invalid = draft.mode === 'size' && (widthBad || heightBad);
  const modes: [WindowMode, string][] = [
    ['default', t('instanceSettings.windowDefault')],
    ['size', t('instanceSettings.windowSize')],
    ['fullscreen', t('instanceSettings.windowFullscreen')],
  ];
  const sizeInput = (key: 'width' | 'height', label: string, bad: boolean) => (
    <div className="grid gap-1.5">
      <label htmlFor={`${id}-${key}`} className="text-sm font-semibold">
        {label}
      </label>
      <input
        id={`${id}-${key}`}
        type="number"
        inputMode="numeric"
        min={key === 'width' ? MIN_WINDOW.width : MIN_WINDOW.height}
        max={MAX_WINDOW_SIDE}
        step={1}
        className="field-input tabular w-28"
        value={draft[key]}
        aria-invalid={bad ? true : undefined}
        aria-describedby={noteId}
        onChange={(e) => onChange({ ...draft, [key]: e.target.value })}
      />
    </div>
  );
  return (
    <fieldset className="grid gap-3">
      <legend className="mb-1.5 text-lg font-bold">{t('instanceSettings.window')}</legend>
      <div className="flex flex-wrap gap-2">
        {modes.map(([mode, label]) => (
          <label key={mode} className="seg" data-checked={draft.mode === mode ? '' : undefined}>
            <input type="radio" name="game-window" className="sr-only" checked={draft.mode === mode} onChange={() => onChange({ ...draft, mode })} />
            {label}
          </label>
        ))}
      </div>
      {draft.mode === 'size' && (
        <div className="flex items-end gap-3">
          {sizeInput('width', t('instanceSettings.windowWidth'), widthBad)}
          <span aria-hidden className="pb-2 text-ink-dim">
            ×
          </span>
          {sizeInput('height', t('instanceSettings.windowHeight'), heightBad)}
        </div>
      )}
      {draft.mode === 'size' && (
        <p id={noteId} className={invalid ? 'text-sm text-coral' : 'text-sm text-ink-dim'}>
          {invalid
            ? t('instanceSettings.windowInvalid', { minWidth: MIN_WINDOW.width, minHeight: MIN_WINDOW.height, max: MAX_WINDOW_SIDE })
            : t('instanceSettings.windowHint', { minWidth: MIN_WINDOW.width, minHeight: MIN_WINDOW.height, max: MAX_WINDOW_SIDE })}
        </p>
      )}
      {draft.mode !== 'default' && <p className="text-sm text-ink-dim">{t('instanceSettings.windowOldVersions')}</p>}
    </fieldset>
  );
}
