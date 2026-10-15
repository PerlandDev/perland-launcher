import { useTranslation } from 'react-i18next';
import { Marquee } from '../../art/Marquee';
import { artSeed } from '../../art/seed';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { useAccounts, useInstances, useInstanceArt } from '../../state/queries';
import { navigate } from '../../state/route';
import { useRun } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Package } from '../../ui/Icon';
import { PlayButton } from '../../ui/PlayButton';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { DamagedView } from './DamagedView';
import { EmptyState } from './EmptyState';
import { InstanceMenu } from './InstanceMenu';
import { ServerPanel } from '../servers/ServerPanel';
import { isPack, isServer, versionLine } from './labels';
import { ManualDownloads } from './ManualDownloads';
import { pick } from './pick';
import { UpdateDialog } from './UpdateDialog';
import { UpdatePlate } from './UpdatePlate';

function InstanceView({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const run = useRun(instance.id);
  const accounts = useAccounts();
  const hasAccount = Boolean(accounts.data?.active);
  const art = useInstanceArt(instance.id);
  // The second click of a double click lands before the `queued` event; hold it until Play settles.
  const playOnce = useSingleFlight();
  return (
    // Home measures its own width (`@container`), not the window's, so the wide rail narrows it honestly.
    <section className="@container relative h-full overflow-hidden" aria-labelledby="instance-name">
      <Marquee seed={artSeed(instance)} attract={run.state === 'idle'} dimmed={run.state !== 'idle'} banner={art.data?.banner ?? null} className="absolute inset-0" />
      {/* Under 55rem (the wide rail at the 960 px minimum) the name takes its own row and the controls sit
          under it at the right, instead of squeezing the name into a column a few letters wide. */}
      <div className="absolute inset-x-0 bottom-0 grid grid-cols-[minmax(0,1fr)_auto] items-end gap-x-4 px-10 pt-6 pb-12 @max-[55rem]:grid-cols-1 @max-[55rem]:gap-y-5">
        {/* The plate sits above both columns, so it reads on one line instead of wrapping in the name's. */}
        {isPack(instance) && (
          <div className="col-span-2 min-w-0 @max-[55rem]:col-span-1">
            <UpdatePlate instance={instance} />
          </div>
        )}
        <div className="min-w-0">
          <h1 id="instance-name" className="marquee-name font-display text-[clamp(2.5rem,5.2cqi,5.5rem)] leading-[0.95] [overflow-wrap:anywhere]">
            {instance.name}
          </h1>
          <p className="marquee-plate mt-3">{versionLine(t, instance)}</p>
          {isServer(instance) && <ServerPanel instance={instance} />}
        </div>
        <div className="flex items-end gap-3 @max-[55rem]:justify-self-end">
          {/* "Mods & packs" is long. Its 1rem pad, the 1rem column gap and the name's 5.2cqi size keep
              an 8-letter name (CREATIVE, SURVIVAL, MODPACKS) whole down to the 55rem side-by-side floor. */}
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
