import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Marquee } from '../../art/Marquee';
import mark from '../../assets/mark.svg';
import { applyLanguage } from '../../i18n';
import type { Instance, Lang } from '../../ipc/schema';
import { useCreateInstance, useSaveSettings, useSettings } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Search } from '../../ui/Icon';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { MicrosoftSignIn } from '../accounts/MicrosoftSignIn';
import { TitleBar } from '../titlebar/TitleBar';
import { useVersionChoice } from '../new-instance/useVersionChoice';
import { VersionFields } from '../new-instance/VersionFields';

const STEPS = 4;

function Steps({ current }: { current: number }) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-2">
      <span className="sr-only">{t('firstRun.step', { n: current + 1, total: STEPS })}</span>
      {Array.from({ length: STEPS }, (_, i) => (
        <span key={i} aria-hidden className={`h-2 w-6 ${i <= current ? 'bg-blue' : 'bg-line'}`} />
      ))}
    </div>
  );
}

export function FirstRun() {
  const { t, i18n } = useTranslation();
  const settings = useSettings();
  const saveSettings = useSaveSettings({ inline: true });
  const create = useCreateInstance({ inline: true });
  const choice = useVersionChoice();
  const [step, setStep] = useState(0);
  const [lang, setLang] = useState<Lang>(i18n.language === 'es' ? 'es' : 'en');
  const [picking, setPicking] = useState(false);
  // Once the instance exists, a retry only saves the settings again: it must never make a second one.
  const [created, setCreated] = useState<Instance | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const shownStep = useRef(step);
  const finishOnce = useSingleFlight();

  // A new step starts at its heading, so keyboard and screen-reader users know where they are.
  // The first screen is left alone: nothing has moved yet.
  useEffect(() => {
    if (shownStep.current === step) return;
    shownStep.current = step;
    heading.current?.focus();
  }, [step]);

  const chooseLang = (next: Lang) => {
    setLang(next);
    applyLanguage(next);
    setStep(2);
  };
  const finish = () => {
    const { minecraft } = choice.resolved;
    const current = settings.data;
    if (!current || (!created && !minecraft)) return;
    finishOnce((done) => {
      const save = (instance: Instance) =>
        saveSettings.mutate(
          { ...current, firstRunDone: true, language: lang },
          { onSuccess: () => navigate({ name: 'home', id: instance.id }), onSettled: done },
        );
      if (created) return save(created);
      create.mutate(
        { name: t('newInstance.namePlaceholder', { version: minecraft }), minecraft: minecraft as string, loader: null },
        {
          onSuccess: (instance) => {
            setCreated(instance);
            save(instance);
          },
          onError: done,
        },
      );
    });
  };

  // Browse instead of a first instance: the same settings save, no instance. The modpack brings its own game version.
  const browse = () => {
    const current = settings.data;
    if (!current) return;
    finishOnce((done) =>
      saveSettings.mutate(
        { ...current, firstRunDone: true, language: lang },
        { onSuccess: () => navigate({ name: 'browse', kind: 'modpack' }), onSettled: done },
      ),
    );
  };

  const latest = choice.list.find((v) => v.kind === 'release')?.id;
  const busy = create.isPending || saveSettings.isPending;

  return (
    <div className="grid h-full grid-rows-[2.25rem_1fr]">
      <TitleBar minimal />
      <main className="relative min-h-0 overflow-hidden">
        <Marquee seed="perland" attract className="absolute inset-0" />
        <div className="relative grid h-full place-items-center p-10">
          <div className="panel grid w-full max-w-lg gap-6 p-8 shadow-[0_8px_0_var(--color-dirt-deep)]">
            {step === 0 && (
              <>
                <img src={mark} alt="" width={112} height={96} />
                <h1 className="font-display text-4xl leading-none">{t('firstRun.welcomeTitle')}</h1>
                <p className="text-ink-dim">{t('firstRun.welcomeBody')}</p>
                <Button variant="primary" size="lg" onClick={() => setStep(1)}>
                  {t('firstRun.start')}
                </Button>
              </>
            )}
            {step === 1 && (
              <>
                <h1 ref={heading} tabIndex={-1} className="font-display text-3xl leading-none outline-none">
                  {t('firstRun.languageTitle')}
                </h1>
                <div className="grid gap-3 sm:grid-cols-2">
                  <Button size="lg" data-pressed={lang === 'en'} aria-pressed={lang === 'en'} onClick={() => chooseLang('en')}>
                    English
                  </Button>
                  <Button size="lg" data-pressed={lang === 'es'} aria-pressed={lang === 'es'} onClick={() => chooseLang('es')}>
                    Español
                  </Button>
                </div>
              </>
            )}
            {step === 2 && (
              <>
                <h1 ref={heading} tabIndex={-1} className="font-display text-3xl leading-none outline-none">
                  {t('firstRun.accountTitle')}
                </h1>
                <MicrosoftSignIn variant="primary" onSignedIn={() => setStep(3)} />
                <div className="grid justify-items-start gap-2">
                  <p className="text-sm text-ink-dim">{t('firstRun.skipLater')}</p>
                  <Button onClick={() => setStep(3)}>{t('firstRun.skip')}</Button>
                </div>
              </>
            )}
            {step === 3 && (
              <>
                <h1 ref={heading} tabIndex={-1} className="font-display text-3xl leading-none outline-none">
                  {t('firstRun.instanceTitle')}
                </h1>
                <p className="text-ink-dim">{t('firstRun.instanceBody')}</p>
                {created ? (
                  <p className="font-semibold">{created.name}</p>
                ) : picking ? (
                  <VersionFields choice={choice} withLoader={false} />
                ) : (
                  <div className="grid gap-2">
                    <p className="font-semibold">{t('firstRun.latestRelease', { version: latest ?? '…' })}</p>
                    <button
                      type="button"
                      className="justify-self-start text-sm text-blue-ink underline underline-offset-4"
                      onClick={() => setPicking(true)}
                    >
                      {t('firstRun.chooseVersion')}
                    </button>
                  </div>
                )}
                {choice.versions.isError && !created && !picking && (
                  <ErrorCard
                    error={choice.versions.error}
                    actions={
                      <Button disabled={choice.versions.isFetching} onClick={() => void choice.versions.refetch()}>
                        {t('common.retry')}
                      </Button>
                    }
                  />
                )}
                {settings.isError && (
                  <ErrorCard
                    error={settings.error}
                    actions={
                      <Button disabled={settings.isFetching} onClick={() => void settings.refetch()}>
                        {t('common.retry')}
                      </Button>
                    }
                  />
                )}
                {(create.isError || saveSettings.isError) && <ErrorCard error={create.error ?? saveSettings.error} />}
                <Button
                  variant="primary"
                  size="lg"
                  disabled={!created && (!choice.resolved.ready || !settings.data)}
                  aria-disabled={busy || undefined}
                  onClick={finish}
                >
                  {t('firstRun.finish')}
                </Button>
                <div className="grid justify-items-start gap-2">
                  <p className="text-sm text-ink-dim">{t('firstRun.browseInstead')}</p>
                  <Button icon={<Search />} disabled={!settings.data} aria-disabled={busy || undefined} onClick={browse}>
                    {t('browse.browseModpacks')}
                  </Button>
                </div>
              </>
            )}
            {step > 0 && <Steps current={step} />}
          </div>
        </div>
      </main>
    </div>
  );
}
