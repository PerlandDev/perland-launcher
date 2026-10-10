import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Accounts } from './features/accounts/Accounts';
import { ActivityDrawer } from './features/activity/ActivityDrawer';
import { CloseGuard } from './features/close-guard/CloseGuard';
import { FirstRun } from './features/first-run/FirstRun';
import { Home } from './features/home/Home';
import { InstanceSettings } from './features/instance-settings/InstanceSettings';
import { Mods } from './features/mods/Mods';
import { NewInstance } from './features/new-instance/NewInstance';
import { Rail } from './features/rail/Rail';
import { Settings } from './features/settings/Settings';
import { TitleBar } from './features/titlebar/TitleBar';
import { applyLanguage } from './i18n';
import { useSettings } from './state/queries';
import { useRoute, type Route } from './state/route';
import { Button } from './ui/Button';
import { ErrorCard } from './ui/ErrorCard';
import { Toasts } from './ui/Toasts';
import { useSingleFlight } from './ui/useSingleFlight';

function Screen({ route }: { route: Route }) {
  switch (route.name) {
    case 'home':
      return <Home id={route.id} />;
    case 'mods':
      return <Mods id={route.id} />;
    case 'instanceSettings':
      return <InstanceSettings id={route.id} />;
    case 'newInstance':
      return <NewInstance />;
    case 'accounts':
      return <Accounts />;
    case 'settings':
      return <Settings />;
  }
}

function Shell() {
  const route = useRoute();
  return (
    <div className="grid h-full grid-rows-[2.25rem_1fr]">
      <TitleBar />
      <div className="grid min-h-0 grid-cols-[4.5rem_1fr]">
        <Rail />
        <main className="relative min-h-0 overflow-hidden pb-9">
          <Screen route={route} />
          <ActivityDrawer />
        </main>
      </div>
    </div>
  );
}

/** The first-run gate depends on the settings, so without them the only safe thing to show is a way to try again. */
function SettingsFailed({ error, retry, busy }: { error: unknown; retry: () => Promise<unknown>; busy: boolean }) {
  const { t } = useTranslation();
  const retryOnce = useSingleFlight();
  return (
    <div className="grid h-full grid-rows-[2.25rem_1fr]">
      <TitleBar />
      <main className="grid place-items-center p-10">
        <div className="w-full max-w-xl">
          <ErrorCard
            error={error}
            actions={
              <Button disabled={busy} onClick={() => retryOnce((done) => void retry().finally(done))}>
                {busy ? t('common.loading') : t('common.retry')}
              </Button>
            }
          />
        </div>
      </main>
    </div>
  );
}

export default function App() {
  const settings = useSettings();
  const language = settings.data?.language ?? null;
  useEffect(() => applyLanguage(language), [language]);

  // A retry puts the query back to pending, so remember the failure to keep the error screen up meanwhile.
  const [failure, setFailure] = useState<unknown>(null);
  if (settings.isError && failure !== settings.error) setFailure(settings.error);
  if (settings.data !== undefined && failure !== null) setFailure(null);

  if (settings.isPending && failure === null) return <div className="h-full bg-ground" aria-busy="true" />;
  return (
    <>
      {settings.data === undefined ? (
        <SettingsFailed error={failure} retry={settings.refetch} busy={settings.isFetching} />
      ) : !settings.data.firstRunDone ? (
        <FirstRun />
      ) : (
        <Shell />
      )}
      <Toasts />
      <CloseGuard />
    </>
  );
}
