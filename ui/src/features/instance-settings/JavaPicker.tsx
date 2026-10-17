import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import type { JavaFound } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { WarningDiamond } from '../../ui/Icon';
import { Toggle } from '../../ui/Toggle';
import type { JavaChoice } from './useJavaChoice';

function JavaOption({ java, checked, onPick, fits }: { java: JavaFound; checked: boolean; onPick: () => void; fits: boolean }) {
  const { t } = useTranslation();
  return (
    <label className="seg seg-row" data-checked={checked ? '' : undefined}>
      <input type="radio" name="java-install" className="sr-only" checked={checked} onChange={onPick} />
      <span className="grid min-w-0 gap-0.5">
        <span>
          {java.vendor ?? 'Java'} {java.version}
          <span className="font-normal text-ink-dim"> · Java {java.major}</span>
          {fits && <span className="font-normal text-ink-dim"> · {t('instanceSettings.javaFits')}</span>}
        </span>
        <span className="selectable truncate font-mono text-xs font-normal text-ink-dim">{java.path}</span>
      </span>
    </label>
  );
}

/** Instance settings → Java: Automatic and the Java it uses, or the Javas found here and Browse. */
export function JavaPicker({ choice }: { choice: JavaChoice }) {
  const { t } = useTranslation();
  const { overview, chosen, path } = choice;
  const required = overview.data?.required ?? null;
  const installs = overview.data?.installs ?? [];
  const unlisted = path !== null && overview.isSuccess && !installs.some((j) => j.path === path);

  const browse = async () => {
    try {
      const picked = await open({ multiple: false, directory: false, title: t('instanceSettings.javaFiles') });
      if (typeof picked === 'string') choice.choose(picked);
    } catch (error: unknown) {
      choice.failPick(error);
    }
  };

  return (
    <fieldset className="grid gap-3">
      <legend className="mb-1.5 text-lg font-bold">{t('instanceSettings.java')}</legend>
      <div className="flex flex-wrap gap-2">
        {(['auto', 'custom'] as const).map((mode) => (
          <label key={mode} className="seg" data-checked={choice.mode === mode ? '' : undefined}>
            <input type="radio" name="java" className="sr-only" checked={choice.mode === mode} onChange={() => choice.setMode(mode)} />
            {mode === 'auto' ? t('instanceSettings.javaAuto') : t('instanceSettings.javaCustom')}
          </label>
        ))}
      </div>
      {choice.mode === 'auto' && (
        <p className="text-sm text-ink-dim">
          {required === null ? t('instanceSettings.javaUsesAutoUnknown') : t('instanceSettings.javaUsesAuto', { major: required })}
        </p>
      )}
      {choice.mode === 'custom' && (
        <div className="grid gap-3">
          {overview.isPending && <p className="text-sm text-ink-dim">{t('instanceSettings.javaSearching')}</p>}
          {overview.isSuccess && installs.length === 0 && !unlisted && <p className="text-sm text-ink-dim">{t('instanceSettings.javaNoneFound')}</p>}
          {overview.isError && <ErrorCard error={overview.error} />}
          {(installs.length > 0 || unlisted) && (
            <div role="radiogroup" aria-label={t('instanceSettings.javaOnComputer')} className="grid gap-3 pb-1">
              {unlisted &&
                (chosen ? (
                  <JavaOption java={chosen} checked onPick={() => {}} fits={chosen.fit === 'matches'} />
                ) : (
                  <p className="selectable truncate font-mono text-sm text-ink-dim">{path}</p>
                ))}
              {installs.map((java) => (
                <JavaOption key={java.path} java={java} checked={java.path === path} onPick={() => choice.choose(java.path)} fits={java.fit === 'matches'} />
              ))}
            </div>
          )}
          <div>
            <Button onClick={() => void browse()}>{t('instanceSettings.javaBrowse')}</Button>
          </div>
          {choice.probe?.isFetching && (
            <p role="status" className="text-sm text-ink-dim">
              {t('instanceSettings.javaChecking')}
            </p>
          )}
          {choice.probe?.isError && <ErrorCard error={choice.probe.error} />}
          {choice.pickError != null && <ErrorCard error={choice.pickError} />}
          {choice.mismatch && chosen && required !== null && (
            <div className="grid gap-3">
              <p role="alert" className="flex items-start gap-2 font-semibold">
                <WarningDiamond className="mt-0.5 h-5 w-5 flex-none text-coral" aria-hidden />
                {t(chosen.fit === 'too_old' ? 'instanceSettings.javaTooOld' : 'instanceSettings.javaNewer', {
                  minecraft: choice.minecraft,
                  required,
                  major: chosen.major,
                })}
              </p>
              <label className="flex items-center gap-3">
                <Toggle checked={choice.acknowledged} onChange={choice.acknowledge} label={t('instanceSettings.javaAnyway')} />
                <span aria-hidden>{t('instanceSettings.javaAnyway')}</span>
              </label>
            </div>
          )}
        </div>
      )}
    </fieldset>
  );
}
