import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Marquee } from '../../art/Marquee';
import { artSeed } from '../../art/seed';
import { pixelUpscale } from '../../art/upscale';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { useAccounts, useAppInfo, useInstances, useInstanceArt, usePlaytime, useSettings } from '../../state/queries';
import { navigate } from '../../state/route';
import { useRun } from '../../state/runStore';
import { openSignInPrompt } from '../../state/signInPrompt';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Login, Package, UserPlus } from '../../ui/Icon';
import { Menu, type MenuHandle } from '../../ui/Menu';
import { PlayButton } from '../../ui/PlayButton';
import { formatPlaytime } from '../../ui/playtime';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { ContinueKey } from './ContinueKey';
import { DamagedView } from './DamagedView';
import { EmptyState } from './EmptyState';
import { useNameFit } from './fitName';
import { useInstanceActions } from './InstanceMenu';
import { ServerPanel } from '../servers/ServerPanel';
import { isPack, isServer, versionLine } from './labels';
import { ManualDownloads } from './ManualDownloads';
import { HeroPreview } from './heroPreview';
import { pick } from './pick';
import { UpdateDialog } from './UpdateDialog';
import { UpdatePlate } from './UpdatePlate';

/** The picture's side inside the icon plate at its largest (the name's full 5.5rem type), in px: what decides
 *  whether a small icon is blown up with hard pixels. */
const PLATE_ART = 72;

/** The instance's own icon (a pack's, or the player's upload), framed beside the name in the shape Settings gives the
 *  rail's icons. It is decoration: the name beside it says what it is. A picture that fails to decode takes the plate
 *  away with it. */
function IconPlate({ src, shape }: { src: string; shape: 'square' | 'round' }) {
  const [failed, setFailed] = useState<string | null>(null);
  const [upscaled, setUpscaled] = useState<string | null>(null);
  if (failed === src) return null;
  return (
    <span className="icon-plate" data-shape={shape}>
      <img
        src={src}
        alt=""
        draggable={false}
        data-upscale={upscaled === src ? '' : undefined}
        onLoad={(e) => {
          const { naturalWidth: width, naturalHeight: height } = e.currentTarget;
          setUpscaled(pixelUpscale({ width, height }, { width: PLATE_ART, height: PLATE_ART }) ? src : null);
        }}
        onError={() => setFailed(src)}
      />
    </span>
  );
}

/** An instance's Home: its marquee, name, controls and dialogs. As a `preview` it is a still copy of the header for
 *  "Adjust the background": no dialogs, and it is not the header that `[data-home-hero]` finds. */
function InstanceView({ instance, preview = false }: { instance: Instance; preview?: boolean }) {
  const { t } = useTranslation();
  const run = useRun(instance.id);
  const accounts = useAccounts();
  const hasAccount = Boolean(accounts.data?.active);
  // Offline accounts exist only in dev builds, where Accounts can add one; anywhere else the way to play is signing in
  // with Microsoft, which the shared prompt offers on the spot.
  const offlineAllowed = useAppInfo().data?.offlineAllowed === true;
  const noAccount = offlineAllowed ? t('play.noAccount') : t('play.signIn');
  const art = useInstanceArt(instance.id);
  const seconds = usePlaytime().data?.find((p) => p.id === instance.id)?.seconds ?? 0;
  const played = formatPlaytime(seconds, t);
  // The background arrives apart from the icon, the player's own picture included.
  const banner = art.data?.banner ?? null;
  // Only a real icon gets a plate: the generated initials would only repeat the name.
  const icon = art.data?.icon ?? null;
  // The plate takes the rail's shape, so the instance looks the same in both places.
  const iconShape = useSettings().data?.iconShape ?? 'square';
  // The second click of a double click lands before the `queued` event; hold it until Play settles.
  const playOnce = useSingleFlight();
  // The row shares the fit, so the icon plate keeps the name's size.
  const nameRef = useNameFit<HTMLHeadingElement>(instance.name, undefined, true);
  const actions = useInstanceActions(instance);
  const menu = useRef<MenuHandle>(null);
  return (
    // Home measures its own width (`@container`), not the window's, so the wide rail narrows it honestly.
    <section
      data-home-hero={preview ? undefined : ''}
      className="@container relative h-full overflow-hidden"
      aria-labelledby="instance-name"
      // A right-click anywhere on the instance opens the ⋯ menu where it was clicked: a shortcut, never the only way.
      onContextMenu={(event) => {
        // The dialogs opened from here render inside it; their fields keep the copy-and-paste menu.
        if (event.target instanceof Element && event.target.closest('dialog')) return;
        event.preventDefault();
        // The keyboard's menu key fires this too, with no point: then the menu opens beside the section's corner.
        menu.current?.openAt(event.clientX || event.clientY ? { x: event.clientX, y: event.clientY } : event.currentTarget);
      }}
    >
      <Marquee
        seed={artSeed(instance)}
        attract={run.state === 'idle'}
        dimmed={run.state !== 'idle'}
        banner={banner}
        focus={instance.bannerFocus}
        zoom={instance.bannerZoom}
        edges={instance.bannerEdges}
        className="absolute inset-0"
      />
      {/* The two things a newcomer looks for first, out in the open at the top right, away from Play's corner.
          They stay raised, never lit, and the ⋯ menu keeps them too, so its list stays whole. */}
      <div className="absolute top-6 right-10 flex gap-3">
        <Button icon={actions.image.icon} disabled={actions.image.disabled} onClick={actions.image.onSelect}>
          {t('image.title')}
        </Button>
        <Button icon={actions.settings.icon} onClick={actions.settings.onSelect}>
          {actions.settings.label}
        </Button>
      </div>
      {/* Under 55rem (the wide rail at the 960 px minimum) the name takes its own row and the controls sit
          under it at the right, instead of squeezing the name into a column a few letters wide. The icon
          plate takes 5.75rem of the name's column, and the name grows with the width, so with a plate the
          name gets its own row under 64rem instead. */}
      <div
        className={`absolute inset-x-0 bottom-0 grid grid-cols-[minmax(0,1fr)_auto] items-end gap-x-4 px-10 pt-6 pb-12 ${icon ? '@max-[64rem]:grid-cols-1 @max-[64rem]:gap-y-5' : '@max-[55rem]:grid-cols-1 @max-[55rem]:gap-y-5'}`}
      >
        {/* The plate sits above both columns, so it reads on one line instead of wrapping in the name's. */}
        {isPack(instance) && (
          <div className={`col-span-2 min-w-0 ${icon ? '@max-[64rem]:col-span-1' : '@max-[55rem]:col-span-1'}`}>
            <UpdatePlate instance={instance} />
          </div>
        )}
        <div className="min-w-0">
          <div className="flex items-end gap-5">
            {icon && <IconPlate src={icon} shape={iconShape} />}
            {/* Sized in app.css: words stay whole, and one too wide for the row shrinks the type. It fills
                the row so useNameFit can read the room it has from its own width. */}
            <h1 ref={nameRef} id="instance-name" className="marquee-name min-w-0 flex-1 font-display leading-[0.95]">
              {instance.name}
            </h1>
          </div>
          {/* The time played rides the version line in the same voice; under a minute it stays out. */}
          <p className="marquee-plate mt-3">
            {played ? `${versionLine(t, instance)} · ${t('home.played', { time: played })}` : versionLine(t, instance)}
          </p>
          {isServer(instance) && <ServerPanel instance={instance} />}
        </div>
        <div className={`flex items-end gap-3 ${icon ? '@max-[64rem]:justify-self-end' : '@max-[55rem]:justify-self-end'}`}>
          {/* "Mods & packs" is long. Its 1rem pad, the 1rem column gap and the name's 5.2cqi size keep
              an 8-letter name (CREATIVE, SURVIVAL, MODPACKS) at full size down to the side-by-side floor (55rem,
              or 64rem when the icon plate shows); a longer word shrinks the type to fit. */}
          <Button size="lg" className="px-4" icon={<Package />} onClick={() => navigate({ name: 'mods', id: instance.id })}>
            {t('home.mods')}
          </Button>
          <Menu ref={menu} label={t('home.more')} items={actions.items} />
          <div className="grid justify-items-end gap-3">
            {/* Why Play is off heads the column (#116): it reads before the keys it explains, and Play's foot lines up
                with the row beside it. A raised key like Continue, never lit, led by a pixel door (sign in) or a pixel
                user-plus (dev builds: add an account). */}
            {!hasAccount && accounts.isSuccess && (
              <Button
                icon={offlineAllowed ? <UserPlus aria-hidden /> : <Login aria-hidden />}
                onClick={() => (offlineAllowed ? navigate({ name: 'accounts' }) : openSignInPrompt())}
              >
                {noAccount}
              </Button>
            )}
            {/* Secondary, above Play and right-aligned to it: one quiet way back to where the player was. */}
            {run.state === 'idle' && <ContinueKey id={instance.id} disabled={!hasAccount} disabledReason={noAccount} once={playOnce} />}
            <PlayButton
              state={run.state}
              progress={run.progress}
              disabled={!hasAccount}
              disabledReason={noAccount}
              onPlay={() => playOnce((done) => void commands.play(instance.id).catch(pushError).finally(done))}
              onStop={() => void commands.stop(instance.id).catch(pushError)}
            />
          </div>
        </div>
      </div>
      {!preview && actions.dialogs}
      {!preview && run.state === 'asking' && run.update && <UpdateDialog instance={instance} update={run.update} />}
      {!preview && run.state === 'idle' && run.lastExit && run.lastExit.manual.length > 0 && (
        <ManualDownloads instance={instance} files={run.lastExit.manual} updateOnly={run.lastExit.updateOnly} />
      )}
    </section>
  );
}

/** A still copy of an instance's Home header, for the background preview (`HeroPreview`). */
export function HeroCopy({ instance }: { instance: Instance }) {
  return <InstanceView instance={instance} preview />;
}

const heroCopy = (instance: Instance) => <HeroCopy instance={instance} />;

export function Home({ id }: { id: string | null }) {
  const listing = useInstances();
  if (listing.isPending) return <div className="h-full bg-panel" aria-busy="true" />;
  const picked = pick(listing.data, id);
  switch (picked.kind) {
    case 'instance':
      return (
        <HeroPreview.Provider value={heroCopy}>
          <InstanceView instance={picked.instance} />
        </HeroPreview.Provider>
      );
    case 'damaged':
      return <DamagedView damaged={picked.damaged} />;
    case 'empty':
      return <EmptyState />;
  }
}

