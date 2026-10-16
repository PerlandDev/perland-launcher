import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Accounts } from './features/accounts/Accounts';
import { ActivityDrawer } from './features/activity/ActivityDrawer';
import { Browse } from './features/browse/Browse';
import { CloseGuard } from './features/close-guard/CloseGuard';
import { FirstRun } from './features/first-run/FirstRun';
import { Home } from './features/home/Home';
import { InstanceSettings } from './features/instance-settings/InstanceSettings';
import { JoinServer } from './features/join/JoinServer';
import { Mods } from './features/mods/Mods';
import { NewInstance } from './features/new-instance/NewInstance';
import { Rail } from './features/rail/Rail';
import { Settings } from './features/settings/Settings';
import { TitleBar } from './features/titlebar/TitleBar';
import { applyLanguage } from './i18n';
import { useActivity } from './state/activity';
import { takeDeepLink } from './state/deepLink';
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
      return <Mods id={route.id} tab={route.tab} />;
    case 'instanceSettings':
      return <InstanceSettings id={route.id} />;
    case 'newInstance':
      return <NewInstance key={route.link ?? 'new'} link={route.link} />;
    case 'browse':
      return <Browse route={route} />;
    case 'joinServer':
      return (
        <JoinServer
          key={`${route.invite}|${route.relinkFor ?? ''}|${route.linkFor ?? ''}`}
          invite={route.invite}
          relinkFor={route.relinkFor}
          linkFor={route.linkFor}
        />
      );
    case 'accounts':
      return <Accounts />;
    case 'settings':
      return <Settings />;
  }
}

function Shell() {
  const route = useRoute();
  const activity = useActivity();
  // The rail is 4.5rem of icons, or 15rem with names when the player widens it; the switch snaps.
  const wide = useSettings().data?.railExpanded ?? false;
  // Once the shell exists (after first run), pick up a link that started the app.
  useEffect(() => void takeDeepLink(), []);
  return (
    <div className="grid h-full grid-rows-[2.25rem_1fr]">
      <TitleBar />
      <div className={`grid min-h-0 ${wide ? 'grid-cols-[15rem_1fr]' : 'grid-cols-[4.5rem_1fr]'}`}>
        <Rail />
        {/* The open drawer is 18rem tall (ActivityDrawer); padding by the same keeps the screen above it. */}
        <main className={`relative min-h-0 overflow-hidden ${activity.open ? 'pb-[18rem]' : 'pb-9'}`}>
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
      <TitleBar minimal />
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

  // A retry puts the query back to pending, so remember a failure to load (never one with data cached) to keep the error screen up meanwhile.
  const [failure, setFailure] = useState<unknown>(null);
  if (settings.isError && settings.data === undefined && failure !== settings.error) setFailure(settings.error);
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
