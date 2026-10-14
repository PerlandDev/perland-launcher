import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { AppInfo, Channel, UpdateState } from '../../ipc/schema';
import { useUpdate } from '../../state/queries';
import { useActiveIds } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Select } from '../../ui/Select';
import { useSingleFlight } from '../../ui/useSingleFlight';

function statusLine(t: TFunction, state: UpdateState, version: string): string {
  switch (state.state) {
    case 'off':
      return t('settings.updates.off');
    case 'idle':
      return t('settings.updates.idle');
    case 'checking':
      return t('settings.updates.checking');
    case 'downloading':
      return state.percent == null
        ? t('settings.updates.downloading')
        : t('settings.updates.downloadingPercent', { percent: state.percent });
    case 'ready':
      return t('settings.updates.ready', { version: state.version });
    case 'up_to_date':
      return t('settings.updates.upToDate', { version });
    case 'failed':
      return t('settings.updates.failed');
  }
}

/** Settings → Launcher updates (spec §4.5). */
export function LauncherUpdates({
  appInfo,
  choice,
  onChannel,
}: {
  appInfo: AppInfo;
  choice: Channel | null;
  onChannel: (channel: Channel) => void;
}) {
  const { t } = useTranslation();
  const update = useUpdate();
  const busy = useActiveIds().length > 0;
  const once = useSingleFlight();
  const state: UpdateState = update.data ?? { state: appInfo.build === 'dev' ? 'off' : 'idle' };
  const run = (action: () => Promise<unknown>) => once((done) => void action().catch(pushError).finally(done));

  if (appInfo.build === 'dev') {
    return (
      <section className="grid gap-2" aria-labelledby="updates-heading">
        <h2 id="updates-heading" className="font-semibold">{t('settings.updates.title')}</h2>
        <p className="text-sm text-ink-dim">{t('settings.updates.off')}</p>
      </section>
    );
  }
  const channel: Channel = choice ?? (appInfo.build === 'beta' ? 'beta' : 'stable');
  const hint =
    channel === 'stable' && appInfo.build === 'beta'
      ? t('settings.updates.stableFromBeta')
      : t(channel === 'beta' ? 'settings.updates.betaHint' : 'settings.updates.stableHint');
  const working = state.state === 'checking' || state.state === 'downloading';
  return (
    <section className="grid gap-3" aria-labelledby="updates-heading">
      <h2 id="updates-heading" className="font-semibold">{t('settings.updates.title')}</h2>
      <Select
        label={t('settings.updates.channel')}
        value={channel}
        options={[
          { value: 'stable', label: t('settings.updates.stable') },
          { value: 'beta', label: t('settings.updates.beta') },
        ]}
        onChange={(value) => onChannel(value as Channel)}
      />
      <p className="text-sm text-ink-dim">{hint}</p>
      <p role="status" className="text-sm">{statusLine(t, state, appInfo.version)}</p>
      <div className="flex flex-wrap items-center gap-3">
        <Button disabled={working} onClick={() => run(() => commands.updateCheck())}>
          {t('settings.updates.check')}
        </Button>
        {state.state === 'ready' && (
          <Button disabled={busy} onClick={() => run(() => commands.updateRestart())}>
            {t('settings.updates.restart')}
          </Button>
        )}
        {state.state === 'ready' && busy && <p className="text-sm text-ink-dim">{t('settings.updates.busy')}</p>}
      </div>
    </section>
  );
}
