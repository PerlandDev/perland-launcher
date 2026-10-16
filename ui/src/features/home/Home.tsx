import { useState, type CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';
import { Marquee } from '../../art/Marquee';
import { artSeed } from '../../art/seed';
import { pixelUpscale } from '../../art/upscale';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { useAccounts, useInstances, useInstanceArt, usePlaytime } from '../../state/queries';
import { navigate } from '../../state/route';
import { useRun } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Package } from '../../ui/Icon';
import { PlayButton } from '../../ui/PlayButton';
import { formatPlaytime } from '../../ui/playtime';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { DamagedView } from './DamagedView';
import { EmptyState } from './EmptyState';
import { InstanceMenu } from './InstanceMenu';
import { widestWordEm } from './nameFit';
import { ServerPanel } from '../servers/ServerPanel';
import { isPack, isServer, versionLine } from './labels';
import { ManualDownloads } from './ManualDownloads';
import { pick } from './pick';
import { UpdateDialog } from './UpdateDialog';
import { UpdatePlate } from './UpdatePlate';

/** The picture's side inside the icon plate (4.5rem less its 0.375rem frame), in px. */
const PLATE_ART = 60;

/** The instance's own icon (a pack's, or the player's upload), framed beside the name. It is decoration:
 *  the name beside it says what it is. A picture that fails to decode takes the plate away with it. */
function IconPlate({ src }: { src: string }) {
  const [failed, setFailed] = useState<string | null>(null);
  const [upscaled, setUpscaled] = useState<string | null>(null);
  if (failed === src) return null;
  return (
    <img
      src={src}
      alt=""
      className="icon-plate"
      draggable={false}
      data-upscale={upscaled === src ? '' : undefined}
      onLoad={(e) => {
        const { naturalWidth: width, naturalHeight: height } = e.currentTarget;
        setUpscaled(pixelUpscale({ width, height }, { width: PLATE_ART, height: PLATE_ART }) ? src : null);
      }}
      onError={() => setFailed(src)}
    />
  );
}

function InstanceView({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const run = useRun(instance.id);
  const accounts = useAccounts();
  const hasAccount = Boolean(accounts.data?.active);
  const art = useInstanceArt(instance.id);
  const seconds = usePlaytime().data?.find((p) => p.id === instance.id)?.seconds ?? 0;
  const played = formatPlaytime(seconds, t);
  // The background arrives apart from the icon, the player's own picture included.
  const banner = art.data?.banner ?? null;
  // Only a real icon gets a plate: the generated initials would only repeat the name.
  const icon = art.data?.icon ?? null;
  // The second click of a double click lands before the `queued` event; hold it until Play settles.
  const playOnce = useSingleFlight();
  return (
    // Home measures its own width (`@container`), not the window's, so the wide rail narrows it honestly.
    <section className="@container relative h-full overflow-hidden" aria-labelledby="instance-name">
      <Marquee
        seed={artSeed(instance)}
        attract={run.state === 'idle'}
        dimmed={run.state !== 'idle'}
        banner={banner}
        focus={instance.bannerFocus}
        className="absolute inset-0"
      />
      {/* Under 55rem (the wide rail at the 960 px minimum) the name takes its own row and the controls sit
          under it at the right, instead of squeezing the name into a column a few letters wide. The icon
          plate takes 5.75rem of the name's column, and the name grows with the width, so with a plate the
          name gets its own row under 64rem instead. */}
      <div
        className={`absolute inset-x-0 bottom-0 grid grid-cols-[minmax(0,1fr)_auto] items-end gap-x-4 px-10 pt-6 pb-12 [--name-size:clamp(2.5rem,5.2cqi,5.5rem)] ${icon ? '@max-[64rem]:grid-cols-1 @max-[64rem]:gap-y-5' : '@max-[55rem]:grid-cols-1 @max-[55rem]:gap-y-5'}`}
      >
        {/* The plate sits above both columns, so it reads on one line instead of wrapping in the name's. */}
        {isPack(instance) && (
          <div className={`col-span-2 min-w-0 ${icon ? '@max-[64rem]:col-span-1' : '@max-[55rem]:col-span-1'}`}>
            <UpdatePlate instance={instance} />
          </div>
        )}
        <div className="min-w-0">
          <div className="flex items-end gap-5">
            {icon && <IconPlate src={icon} />}
            {/* The name's own column is a container, so the size can also shrink until the widest
                word fits it. Only a word too long even at the 1.5rem floor still breaks. */}
            <div className="@container/name min-w-0 flex-1">
              <h1
                id="instance-name"
                className="marquee-name font-display text-[max(1.5rem,min(var(--name-size),calc(100cqi/var(--name-widest))))] leading-[0.95] [overflow-wrap:break-word]"
                style={{ '--name-widest': String(widestWordEm(instance.name)) } as CSSProperties}
              >
                {instance.name}
              </h1>
            </div>
          </div>
          {/* The time played rides the version line in the same voice; under a minute it stays out. */}
          <p className="marquee-plate mt-3">
            {played ? `${versionLine(t, instance)} · ${t('home.played', { time: played })}` : versionLine(t, instance)}
          </p>
          {isServer(instance) && <ServerPanel instance={instance} />}
        </div>
        <div className={`flex items-end gap-3 ${icon ? '@max-[64rem]:justify-self-end' : '@max-[55rem]:justify-self-end'}`}>
          {/* "Mods & packs" is long. Its 1rem pad, the 1rem column gap and the name's 5.2cqi size keep
              an 8-letter name (CREATIVE, SURVIVAL, MODPACKS) whole down to the side-by-side floor (55rem, or 64rem when the icon plate shows). */}
          <Button size="lg" className="px-4" icon={<Package />} onClick={() => navigate({ name: 'mods', id: instance.id })}>
            {t('home.mods')}
          </Button>
          <InstanceMenu instance={instance} />
          <div className="grid justify-items-end gap-3">
            <PlayButton
              state={run.state}
              progress={run.progress}
              disabled={!hasAccount}
              disabledReason={t('play.noAccount')}
              onPlay={() => playOnce((done) => void commands.play(instance.id).catch(pushError).finally(done))}
              onStop={() => void commands.stop(instance.id).catch(pushError)}
            />
            {!hasAccount && accounts.isSuccess && (
              <button type="button" className="text-sm text-blue-ink underline underline-offset-4" onClick={() => navigate({ name: 'accounts' })}>
                {t('play.noAccount')}
              </button>
            )}
          </div>
        </div>
      </div>
      {run.state === 'asking' && run.update && <UpdateDialog instance={instance} update={run.update} />}
      {run.state === 'idle' && run.lastExit && run.lastExit.manual.length > 0 && (
        <ManualDownloads instance={instance} files={run.lastExit.manual} updateOnly={run.lastExit.updateOnly} />
      )}
    </section>
  );
}

export function Home({ id }: { id: string | null }) {
  const listing = useInstances();
  if (listing.isPending) return <div className="h-full bg-panel" aria-busy="true" />;
  const picked = pick(listing.data, id);
  switch (picked.kind) {
    case 'instance':
      return <InstanceView instance={picked.instance} />;
    case 'damaged':
      return <DamagedView damaged={picked.damaged} />;
    case 'empty':
      return <EmptyState />;
  }
}
