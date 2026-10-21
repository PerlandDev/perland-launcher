import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { Field } from '../../ui/Field';
import { ChevronDown } from '../../ui/Icon';
import { SwitchRow } from '../../ui/SwitchRow';
import { DIFFICULTIES, GAME_MODES, modeLabel, oneLine, type Difficulty, type ServerOptionsModel } from './serverOptions';

// The Options disclosure of the free-server dialogs; its state and rules are in serverOptions.ts.

/** The disclosure and, open, the fields. Renders nothing when the Engine doesn't take settings. */
export function ServerOptions({ options }: { options: ServerOptionsModel }) {
  const { t } = useTranslation();
  const id = useId();
  const { show, state, set, open, setOpen, errors, limits } = options;
  if (!show || !limits.settings) return null;
  const hardcore = state.gameMode === 'hardcore';
  const shownDifficulty: Difficulty = hardcore ? 'hard' : state.difficulty;
  const modes = GAME_MODES.filter((m) => limits.settings?.gameModes.includes(m));
  const difficulties = DIFFICULTIES.filter((d) => limits.settings?.difficulties.includes(d) || d === shownDifficulty);
  const summary = t('freeServer.options.summary', {
    mode: modeLabel(t, state.gameMode),
    difficulty: t(`manage.settings.difficulties.${shownDifficulty}`),
    count: Number(state.maxPlayers) || state.limit,
  });
  const line = (field: string) =>
    errors[field] ? (
      <p role="alert" className="text-sm text-coral">
        {errors[field]}
      </p>
    ) : null;
  return (
    <div className="grid gap-5">
      <button type="button" className="options-disclosure" aria-expanded={open} aria-controls={open ? id : undefined} onClick={() => setOpen(!open)}>
        <span className="grid min-w-0 gap-0.5">
          <span className="text-sm font-semibold text-ink">{t('freeServer.options.title')}</span>
          {!open && <span className="text-sm text-ink-dim">{summary}</span>}
        </span>
        <ChevronDown className={`h-5 w-5 flex-none${open ? ' rotate-180' : ''}`} aria-hidden />
      </button>
      {open && (
        <div id={id} className="grid gap-5">
          <fieldset className="grid gap-1.5">
            <legend className="mb-1.5 text-sm font-semibold">{t('manage.settings.mode')}</legend>
            <div className="flex flex-wrap gap-2">
              {modes.map((mode) => (
                <label key={mode} className="seg text-ink" data-checked={state.gameMode === mode ? '' : undefined}>
                  <input
                    type="radio"
                    name={`${id}-mode`}
                    className="sr-only"
                    checked={state.gameMode === mode}
                    onChange={() => set({ gameMode: mode })}
                  />
                  {modeLabel(t, mode)}
                </label>
              ))}
            </div>
            {/* Below the segments' 4px travel, as the kind's hint. */}
            <div className="mt-1">{line('settings.gameMode')}</div>
          </fieldset>
          <div className="manage-pair">
            <Field
              label={t('manage.settings.difficulty')}
              hint={hardcore ? t('freeServer.options.hardcoreHint') : null}
              error={errors['settings.difficulty']}
              alert
            >
              <select
                className="field-input"
                value={shownDifficulty}
                disabled={hardcore}
                onChange={(e) => set({ difficulty: e.target.value as Difficulty })}
              >
                {difficulties.map((d) => (
                  <option key={d} value={d}>
                    {t(`manage.settings.difficulties.${d}`)}
                  </option>
                ))}
              </select>
            </Field>
            <Field
              label={t('manage.settings.maxPlayers')}
              hint={t('freeServer.options.maxPlayersHint', { max: state.limit })}
              error={errors['settings.maxPlayers']}
              alert
            >
              <input
                type="number"
                inputMode="numeric"
                min={1}
                max={state.limit}
                step={1}
                className="field-input w-32 tabular text-ink"
                value={state.maxPlayers}
                onChange={(e) => set({ maxPlayers: e.target.value })}
              />
            </Field>
          </div>
          <SwitchRow
            label={t('manage.settings.pvp')}
            hint={t('manage.settings.pvpHint')}
            checked={state.pvp}
            onChange={(pvp) => set({ pvp })}
            after={line('settings.pvp')}
          />
          <SwitchRow
            label={t('manage.settings.whitelist')}
            hint={t('freeServer.options.whitelistHint')}
            checked={state.whitelist}
            onChange={(whitelist) => set({ whitelist })}
            after={line('settings.whitelist')}
          />
          <Field label={t('freeServer.options.seed')} hint={t('freeServer.options.seedHint')} error={errors['settings.seed']} alert>
            <input
              className="field-input text-ink"
              value={state.seed}
              maxLength={limits.settings.seedMaxBytes}
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => set({ seed: oneLine(e.target.value) })}
            />
          </Field>
          <Field label={t('freeServer.options.motd')} hint={t('freeServer.options.motdHint')} error={errors['settings.motd']} alert>
            <input
              className="field-input text-ink"
              value={state.motd}
              maxLength={limits.settings.motdMaxBytes}
              autoComplete="off"
              onChange={(e) => set({ motd: oneLine(e.target.value) })}
            />
          </Field>
        </div>
      )}
    </div>
  );
}
