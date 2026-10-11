import { useTranslation } from 'react-i18next';
import { Marquee } from '../../art/Marquee';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { useAccounts, useInstances, usePackArt } from '../../state/queries';
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
import { isPack, versionLine } from './labels';
import { ManualDownloads } from './ManualDownloads';
import { pick } from './pick';
import { UpdateDialog } from './UpdateDialog';
import { UpdatePlate } from './UpdatePlate';

function InstanceView({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const run = useRun(instance.id);
  const accounts = useAccounts();
  const hasAccount = Boolean(accounts.data?.active);
  const art = usePackArt(instance.id, isPack(instance));
  // The second click of a double click lands before the `queued` event; hold it until Play settles.
  const playOnce = useSingleFlight();
  return (
    <section className="relative h-full overflow-hidden" aria-labelledby="instance-name">
      <Marquee seed={instance.id} attract={run.state === 'idle'} dimmed={run.state !== 'idle'} banner={art.data?.banner ?? null} className="absolute inset-0" />
      <div className="absolute inset-x-0 bottom-0 grid grid-cols-[minmax(0,1fr)_auto] items-end gap-x-10 px-10 pt-6 pb-12">
        {/* The plate sits above both columns, so it reads on one line instead of wrapping in the name's. */}
        {isPack(instance) && (
          <div className="col-span-2 min-w-0">
            <UpdatePlate instance={instance} />
          </div>
        )}
        <div className="min-w-0">
          <h1 id="instance-name" className="marquee-name font-display text-[clamp(2.5rem,5.4vw,5.5rem)] leading-[0.95] [overflow-wrap:anywhere]">
            {instance.name}
          </h1>
          <p className="marquee-plate mt-3">{versionLine(t, instance)}</p>
        </div>
        <div className="flex items-end gap-3">
          <Button size="lg" icon={<Package />} onClick={() => navigate({ name: 'mods', id: instance.id })}>
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
