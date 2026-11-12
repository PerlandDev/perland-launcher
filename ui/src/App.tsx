import { lazy, Suspense, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { SignInPrompt } from './features/accounts/SignInPrompt';
import { TrustDialog } from './features/trust/TrustDialog';
import { ActivityDrawer } from './features/activity/ActivityDrawer';
import { Browse } from './features/browse/Browse';
import { CloseGuard } from './features/close-guard/CloseGuard';
import { FirstRun } from './features/first-run/FirstRun';
import { HeroCopy, Home } from './features/home/Home';
import { HeroPreview } from './features/home/heroPreview';
import { InstanceSettings } from './features/instance-settings/InstanceSettings';
import { JoinServer } from './features/join/JoinServer';
import { ManageScreen } from './features/manage/ManageScreen';
import { AchievementsScreen } from './features/medals/AchievementsScreen';
import { Mods } from './features/mods/Mods';
import { NewInstance } from './features/new-instance/NewInstance';
import { Profile } from './features/profile/Profile';
import { Rail } from './features/rail/Rail';
import { ServersScreen } from './features/servers-list/ServersScreen';
import { Settings } from './features/settings/Settings';
import { TitleBar } from './features/titlebar/TitleBar';
import { WhatsNewOnLaunch } from './features/whats-new/WhatsNewOnLaunch';
import { applyLanguage } from './i18n';
import { useActivity } from './state/activity';
import { applyAppearance } from './state/appearance';
import { takeDeepLink } from './state/deepLink';
import { useSettings } from './state/queries';
import type { Instance } from './ipc/schema';
import { useRoute, type Route } from './state/route';
import { Button } from './ui/Button';
import { ErrorCard } from './ui/ErrorCard';
import { Toasts } from './ui/Toasts';
import { useSingleFlight } from './ui/useSingleFlight';

// three.js stays out of the start bundle: the Skins screen loads on first visit.
const SkinStudio = lazy(() => import('./features/skins/SkinStudio'));

/** Home's header as a still copy, for the background preview opened from anywhere (the rail's menu too). */
const heroCopy = (instance: Instance) => <HeroCopy instance={instance} />;

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
    case 'servers':
      return <ServersScreen />;
    case 'manage':
      return <ManageScreen key={`${route.tenantId}/${route.stack}`} tenantId={route.tenantId} stack={route.stack} />;
    case 'skins':
      return (
        <Suspense fallback={null}>
          <SkinStudio skin={route.skin} account={route.account} />
        </Suspense>
      );
    case 'achievements':
      return <AchievementsScreen />;
    case 'settings':
      return <Settings />;
    case 'profile':
      return <Profile add={route.add} />;
  }
}

/** What makes a move: another screen, or another instance on the same one. */
const placeKey = (route: Route) => `${route.name}:${'id' in route ? (route.id ?? '') : ''}`;

/** 0 or 1, flipping each time `key` changes. */
function useBeat(key: string): 0 | 1 {
  const [seen, setSeen] = useState<{ key: string; beat: 0 | 1 }>({ key, beat: 0 });
  if (seen.key !== key) setSeen({ key, beat: seen.beat === 0 ? 1 : 0 });
  return seen.key === key ? seen.beat : seen.beat === 0 ? 1 : 0;
}

function Shell() {
  const route = useRoute();
  const activity = useActivity();
  // The rail is 4.5rem of icons, or 15rem with names when the player widens it; the switch snaps.
  const settings = useSettings().data;
  const wide = settings?.railExpanded ?? false;
  const beat = useBeat(placeKey(route));
  // Once the shell exists (after first run), pick up a link that started the app.
  useEffect(() => void takeDeepLink(), []);
  return (
    // The rail's menu opens Change image too, so its background preview can draw Home's header from anywhere.
    <HeroPreview.Provider value={heroCopy}>
      <div className="grid h-full grid-rows-[2.75rem_1fr]">
        <TitleBar />
        <div className={`grid min-h-0 ${wide ? 'grid-cols-[15rem_1fr]' : 'grid-cols-[4.5rem_1fr]'}`}>
          <Rail />
          {/* The open drawer is 18rem tall (ActivityDrawer); padding by the same keeps the screen above it.
              `overflow-clip`, not `hidden`: a hidden box still scrolls to show a focused element, which would slide
              the screen and the drawer up and leave the window half empty (#121). Screens scroll inside themselves. */}
          <main className={`relative min-h-0 overflow-clip ${activity.open ? 'pb-[18rem]' : 'pb-9'}`}>
            {/* Each place arrives (app.css, "Arrivals") in the style Settings give: eased (the default), the arcade
                cabinet's own snapped frames, or not at all. The beat flips at every move, which replays the entrance without building the
                screen again. */}
            <div className="arrive" data-style={settings?.transitions ?? 'smooth'} data-arrive={route.name === 'home' ? 'home' : 'screen'} data-beat={beat}>
              <Screen route={route} />
            </div>
            <ActivityDrawer />
          </main>
        </div>
      </div>
    </HeroPreview.Provider>
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
  // Until the settings arrive, the look main.tsx applied from the mirror stays.
  const theme = settings.data?.theme;
  const font = settings.data?.font;
  useEffect(() => {
    if (theme && font) applyAppearance({ theme, font });
  }, [theme, font]);

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
      <SignInPrompt />
      <TrustDialog />
      <CloseGuard />
      {settings.data !== undefined && <WhatsNewOnLaunch firstRunDone={settings.data.firstRunDone} />}
    </>
  );
}
