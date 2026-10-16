import { openUrl } from '@tauri-apps/plugin-opener';
import { useTranslation } from 'react-i18next';
import mark from '../../assets/mark.svg';
import { applyLanguage } from '../../i18n';
import { commands } from '../../ipc/commands';
import type { Lang } from '../../ipc/schema';
import { applyAppearance, FONTS, THEMES, type Appearance } from '../../state/appearance';
import { useAppInfo, useSettings } from '../../state/queries';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { ExternalLink, Folder } from '../../ui/Icon';
import { MemorySlider } from '../../ui/MemorySlider';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { Select } from '../../ui/Select';
import { Toggle } from '../../ui/Toggle';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { Diagnostics } from './Diagnostics';
import { LauncherUpdates } from './LauncherUpdates';
import { useSettingsDraft } from './useSettingsDraft';

const SOURCE = 'https://github.com/PerlandDev/perland-launcher';
const LICENSE = 'https://www.gnu.org/licenses/gpl-3.0.html';

export function Settings() {
  const { t } = useTranslation();
  const settings = useSettings();
  const appInfo = useAppInfo();
  const { draft, saved, edit } = useSettingsDraft();
  const openOnce = useSingleFlight();
  const retryOnce = useSingleFlight();
  const data = draft ?? settings.data;

  /** A look applies to the whole app at once, before the save comes back. */
  const look = (patch: Partial<Appearance>) => {
    if (!data) return;
    applyAppearance({ theme: data.theme, font: data.font, ...patch });
    edit(patch);
  };

  const open = (action: () => Promise<unknown>) => openOnce((done) => void action().catch(pushError).finally(done));

  if (settings.isError || appInfo.isError) {
    const retry = () =>
      retryOnce(
        (done) =>
          void Promise.all([settings.isError && settings.refetch(), appInfo.isError && appInfo.refetch()]).finally(done),
      );
    return (
      <ScreenFrame title={t('settings.title')}>
        <div className="max-w-2xl">
          <ErrorCard error={settings.error ?? appInfo.error} actions={<Button onClick={retry}>{t('common.retry')}</Button>} />
        </div>
      </ScreenFrame>
    );
  }
  if (!data || !appInfo.data) return <ScreenFrame title={t('settings.title')}>{null}</ScreenFrame>;

  return (
    <ScreenFrame
      title={t('settings.title')}
      // A live region, so a screen reader hears that a change went through.
      actions={
        <p role="status" className="text-sm text-ink-dim">
          {draft ? t('common.saving') : saved ? t('common.saved') : ''}
        </p>
      }
    >
      <div className="grid max-w-2xl gap-10">
        <Select
          label={t('settings.language')}
          value={data.language ?? 'system'}
          options={[
            { value: 'system', label: t('settings.languageSystem') },
            { value: 'en', label: t('settings.english') },
            { value: 'es', label: t('settings.spanish') },
          ]}
          onChange={(value) => {
            const language = value === 'system' ? null : (value as Lang);
            applyLanguage(language);
            edit({ language });
          }}
        />
        <MemorySlider
          label={t('settings.memory')}
          valueMb={data.defaultMemoryMb}
          systemMb={appInfo.data.systemMemoryMb}
          onChange={(defaultMemoryMb) => edit({ defaultMemoryMb }, true)}
        />
        <div className="flex items-start justify-between gap-6">
          <div>
            <p className="font-semibold" aria-hidden>
              {t('settings.minimize')}
            </p>
            <p className="text-sm text-ink-dim">{t('settings.minimizeHint')}</p>
          </div>
          <Toggle checked={data.minimizeOnPlay} label={t('settings.minimize')} onChange={(minimizeOnPlay) => edit({ minimizeOnPlay })} />
        </div>
        <section className="grid gap-4" aria-labelledby="appearance-heading">
          <h2 id="appearance-heading" className="text-lg font-bold">
            {t('settings.appearance')}
          </h2>
          <div className="grid gap-6">
            <fieldset role="radiogroup" className="grid gap-2">
              <legend className="text-sm font-semibold">{t('settings.theme')}</legend>
              <div className="swatch-keys">
                {THEMES.map((theme) => (
                  <label key={theme} className="swatch-key" data-checked={data.theme === theme ? '' : undefined}>
                    <input
                      type="radio"
                      name="theme"
                      className="sr-only"
                      checked={data.theme === theme}
                      onChange={() => look({ theme })}
                    />
                    <span className="swatch" data-swatch={theme} aria-hidden>
                      <span className="swatch-face" />
                      <span className="swatch-lamp" />
                    </span>
                    <span className="swatch-name">{t(`settings.themes.${theme}`)}</span>
                  </label>
                ))}
              </div>
            </fieldset>
            <fieldset className="grid gap-2">
              <legend className="text-sm font-semibold">{t('settings.font')}</legend>
              <div className="flex flex-wrap gap-2">
                {FONTS.map((font) => (
                  // Each option is set in the face it picks, so the choice shows itself.
                  <label
                    key={font}
                    className={`seg ${font === 'system' ? 'font-system' : 'font-hyperlegible'}`}
                    data-checked={data.font === font ? '' : undefined}
                  >
                    <input
                      type="radio"
                      name="font"
                      className="sr-only"
                      checked={data.font === font}
                      onChange={() => look({ font })}
                    />
                    {font === 'system' ? t('settings.fontSystem') : t('settings.fontHyperlegible')}
                  </label>
                ))}
              </div>
            </fieldset>
            <fieldset className="grid gap-2">
              <legend className="text-sm font-semibold">{t('settings.iconShape')}</legend>
              <div className="flex flex-wrap gap-2">
                {(['square', 'round'] as const).map((shape) => (
                  <label key={shape} className="seg" data-checked={data.iconShape === shape ? '' : undefined}>
                    <input
                      type="radio"
                      name="icon-shape"
                      className="sr-only"
                      checked={data.iconShape === shape}
                      onChange={() => edit({ iconShape: shape })}
                    />
                    {shape === 'square' ? t('settings.iconSquare') : t('settings.iconRound')}
                  </label>
                ))}
              </div>
            </fieldset>
          </div>
        </section>
        <div className="grid gap-2">
          <p className="text-sm font-semibold">{t('settings.dataFolder')}</p>
          <div className="flex items-center gap-3">
            <p className="selectable min-w-0 flex-1 truncate font-mono text-sm text-ink-dim">{appInfo.data.dataDir}</p>
            <Button icon={<Folder />} onClick={() => open(() => commands.openFolder({ kind: 'data' }))}>
              {t('common.openFolder')}
            </Button>
          </div>
        </div>
        <LauncherUpdates appInfo={appInfo.data} choice={data.updateChannel} onChannel={(updateChannel) => edit({ updateChannel })} />
        <Diagnostics />
        <section className="panel grid gap-4 p-6" aria-labelledby="about-heading">
          <div className="flex items-center gap-4">
            <img src={mark} alt="" width={56} height={48} />
            <div>
              <h2 id="about-heading" className="font-display text-xl">{t('app.name')}</h2>
              <p className="text-sm text-ink-dim">
                {t('settings.version', { version: appInfo.data.version })}
                {appInfo.data.build === 'dev' && ` · ${t('settings.devBuild')}`}
                {appInfo.data.build === 'beta' && ` · ${t('settings.betaBuild')}`}
              </p>
            </div>
          </div>
          <p>{t('settings.license')}</p>
          <div className="flex flex-wrap gap-3">
            <Button icon={<ExternalLink />} onClick={() => open(() => openUrl(LICENSE))}>
              {t('settings.licenseLink')}
            </Button>
            <Button icon={<ExternalLink />} onClick={() => open(() => openUrl(SOURCE))}>
              {t('settings.source')}
            </Button>
          </div>
          <p className="text-sm text-ink-dim">{t('settings.notOfficial')}</p>
        </section>
      </div>
    </ScreenFrame>
  );
}
