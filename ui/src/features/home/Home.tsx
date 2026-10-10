import { useTranslation } from 'react-i18next';
import { Marquee } from '../../art/Marquee';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { useAccounts, useInstances } from '../../state/queries';
import { navigate } from '../../state/route';
import { useRun } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Package } from '../../ui/Icon';
import { PlayButton } from '../../ui/PlayButton';
import { DamagedView } from './DamagedView';
import { EmptyState } from './EmptyState';
import { InstanceMenu } from './InstanceMenu';
import { versionLine } from './labels';
import { pick } from './pick';

function InstanceView({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const run = useRun(instance.id);
  const accounts = useAccounts();
  const hasAccount = Boolean(accounts.data?.active);
  return (
    <section className="relative h-full overflow-hidden" aria-labelledby="instance-name">
      <Marquee seed={instance.id} attract={run.state === 'idle'} className="absolute inset-0 h-full w-full" />
      <div className="absolute inset-x-0 bottom-0 flex flex-wrap items-end justify-between gap-x-10 gap-y-6 px-10 pt-6 pb-12">
        <div className="min-w-0 max-w-[60ch]">
          <h1 id="instance-name" className="font-display text-[clamp(2.5rem,5.4vw,5.5rem)] leading-[0.95]">
            {instance.name}
          </h1>
          <p className="mt-3 text-ink-dim">{versionLine(t, instance)}</p>
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
              onPlay={() => void commands.play(instance.id).catch(pushError)}
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
