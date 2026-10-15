import type { i18n as I18n } from 'i18next';
import { useQueryClient } from '@tanstack/react-query';
import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { describeError } from '../../ipc/errors';
import type { AppInfo, Channel, UpdateState } from '../../ipc/schema';
import { qk, useUpdate } from '../../state/queries';
import { pushError } from '../../state/toasts';
import { useUpdateBusy } from '../../state/updateBusy';
import { Button } from '../../ui/Button';
import { Reload } from '../../ui/Icon';
import { Meter } from '../../ui/Meter';
import { useSingleFlight } from '../../ui/useSingleFlight';

/** The plate's blue lamp is lit while an update is on its way (checking, downloading, ready).
 *  Up to date gets no lamp: on this screen lime marks only the chosen channel. */
const lampOf = (state: UpdateState) =>
  state.state === 'checking' || state.state === 'downloading' || state.state === 'ready' ? 'info' : 'none';

/** The state in words, and the way forward when there is one. The percent is not here: it ticks,
 *  and this is the live region, so a screen reader hears each state once rather than every percent. */
function statusText(i18n: I18n, state: UpdateState, version: string): { line: string; fix: string | null } {
  const t = i18n.t;
  switch (state.state) {
    case 'off':
      return { line: t('settings.updates.off'), fix: null };
    case 'idle':
      return { line: t('settings.updates.idle'), fix: null };
    case 'checking':
      return { line: t('settings.updates.checking'), fix: null };
    case 'downloading':
      return { line: t('settings.updates.downloading'), fix: null };
    case 'ready':
      return { line: t('settings.updates.ready', { version: state.version }), fix: null };
    case 'up_to_date':
      return { line: t('settings.updates.upToDate', { version }), fix: null };
    case 'failed': {
      // The error's own words: a download that failed its safety check is not "couldn't check".
      const { message, fix } = describeError(i18n, state.error);
      return { line: message, fix };
    }
  }
}

/** Settings → Launcher updates (spec §4.5). Nothing here is lit: these and About's buttons are all
 *  raised, so no two primaries compete. Blue is progress; lime is only the channel you're on. */
export function LauncherUpdates({
  appInfo,
  choice,
  onChannel,
}: {
  appInfo: AppInfo;
  choice: Channel | null;
  onChannel: (channel: Channel) => void;
}) {
  const { t, i18n } = useTranslation();
  const update = useUpdate();
  const queryClient = useQueryClient();
  const busy = useUpdateBusy();
  const once = useSingleFlight();
  const ids = useId();
  const state: UpdateState = update.data ?? { state: appInfo.build === 'dev' ? 'off' : 'idle' };
  const run = (action: () => Promise<unknown>) => once((done) => void action().catch(pushError).finally(done));

  if (appInfo.build === 'dev') {
    return (
      <section className="grid gap-2" aria-labelledby="updates-heading">
        <h2 id="updates-heading" className="text-lg font-bold">
          {t('settings.updates.title')}
        </h2>
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
  const ready = state.state === 'ready';
  const { line, fix } = statusText(i18n, state, appInfo.version);
  const percent = state.state === 'downloading' ? state.percent : null;
  const hintId = `${ids}-hint`;
  const busyId = `${ids}-busy`;
  return (
    <section className="grid gap-4" aria-labelledby="updates-heading">
      <h2 id="updates-heading" className="text-lg font-bold">
        {t('settings.updates.title')}
      </h2>
      <fieldset className="grid gap-1.5" aria-describedby={hintId}>
        <legend className="mb-1.5 text-sm font-semibold">{t('settings.updates.channel')}</legend>
        <div className="flex flex-wrap gap-2">
          {(['stable', 'beta'] as const).map((value) => (
            <label key={value} className="seg" data-checked={channel === value ? '' : undefined}>
              <input
                type="radio"
                name="update-channel"
                className="sr-only"
                checked={channel === value}
                onChange={() => onChannel(value)}
              />
              {t(value === 'beta' ? 'settings.updates.beta' : 'settings.updates.stable')}
            </label>
          ))}
        </div>
        {/* 0.25rem below the segments' 4px travel, so the hint never touches a pressed segment. */}
        <p id={hintId} className="mt-1 text-sm text-ink-dim">
          {hint}
        </p>
      </fieldset>
      <div className="note-plate" data-lamp={lampOf(state)}>
        <div className="grid min-w-0 flex-1 gap-2">
          <p role="status" className={state.state === 'idle' ? 'text-ink-dim' : undefined}>
            {line}
          </p>
          {fix && <p className="text-sm text-ink-dim">{fix}</p>}
          {percent != null && (
            <div className="flex items-center gap-3">
              <div className="min-w-0 flex-1">
                <Meter value={percent / 100} label={t('settings.updates.downloading')} />
              </div>
              <span className="tabular text-sm font-semibold" aria-hidden>
                {t('settings.updates.percent', { percent })}
              </span>
            </div>
          )}
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        {ready && (
          <Button
            disabled={busy}
            aria-describedby={busy ? busyId : undefined}
            onClick={() => run(() => commands.updateRestart())}
          >
            {t('settings.updates.restart')}
          </Button>
        )}
        <Button
          icon={<Reload />}
          disabled={working}
          onClick={() => run(async () => queryClient.setQueryData(qk.update, await commands.updateCheck()))}
        >
          {t('settings.updates.check')}
        </Button>
        {ready && busy && (
          <p id={busyId} className="text-sm text-ink-dim">
            {t('settings.updates.busy')}
          </p>
        )}
      </div>
    </section>
  );
}
