import { useState } from 'react';
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import { InstanceIcon } from '../../art/InstanceIcon';
import type { ManageCause, ManageDetail, ServerStatus } from '../../ipc/schema';
import { openManagePanel, useManageAction, useManageDetail, useManageDiagnosis } from '../../state/queries';
import { Button } from '../../ui/Button';
import { CopyRow } from '../../ui/CopyRow';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { ExternalLink, WarningDiamond } from '../../ui/Icon';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { StatusPill } from '../servers/StatusPill';
import { causeLine } from './causeLine';

type Action = 'start' | 'stop' | 'restart';

/** The state in words, and the LED it rides: lime running, blue while it changes, a switched-off lamp asleep, coral stopped. */
function stateView(t: TFunction, detail: ManageDetail, crashed: boolean): { word: string; led: ServerStatus | null } {
  switch (detail.state) {
    case 'running':
      return {
        word: detail.online === null ? t('manage.state.runningNoCount') : t('manage.state.running', { count: detail.online }),
        led: 'online',
      };
    case 'asleep':
      return { word: t('manage.state.asleep'), led: 'sleeping' };
    case 'starting':
    case 'stopping':
      return { word: t(`manage.state.${detail.state}`), led: 'starting' };
    case 'stopped':
      return { word: crashed ? t('manage.state.crashed') : t('manage.state.stopped'), led: 'offline' };
    default:
      return { word: t('manage.state.unknown'), led: null };
  }
}

/** The top of Manage: the server's art and state in words, the one big button the state allows, the address and, when the
 *  last run failed, why. Start is the screen's one lit button; Stop is coral like Play while a game runs, Restart raised, and
 *  both ask first because they send everyone out. Nothing to press while it starts or stops. */
export function ManageOverview({ tenantId, stack }: { tenantId: string; stack: string }) {
  const { t } = useTranslation();
  const detail = useManageDetail(tenantId, stack);
  const state = detail.data?.state;
  const diagnosis = useManageDiagnosis(tenantId, stack, state === 'stopped');
  const action = useManageAction(tenantId, stack, { inline: true });
  const [asking, setAsking] = useState<'stop' | 'restart' | null>(null);
  const retryOnce = useSingleFlight();
  const panelOnce = useSingleFlight();

  if (detail.isError && !detail.data) {
    return (
      <ErrorCard
        error={detail.error}
        actions={<Button onClick={() => retryOnce((done) => void detail.refetch().finally(done))}>{t('common.retry')}</Button>}
      />
    );
  }

  const run = (which: Action) => {
    if (action.isPending) return;
    setAsking(null);
    action.mutate(which);
  };
  const failed = state === 'stopped' && diagnosis.data?.failed === true;
  const cause: ManageCause | null =
    diagnosis.data?.cause ?? (diagnosis.data?.oomKilled ? { code: 'out_of_memory', known: true, subject: null, params: {} } : null);
  const view = detail.data ? stateView(t, detail.data, failed) : null;
  const pending = action.isPending ? action.variables : null;
  const busyWord = (which: Action) => (which === 'start' ? t('manage.action.starting') : which === 'stop' ? t('manage.action.stopping') : t('manage.action.restarting'));

  return (
    <section className="panel manage-head" aria-label={stack} aria-busy={!detail.data || undefined}>
      <div className="manage-head-top">
        <div className="manage-icon" aria-hidden>
          <InstanceIcon name={stack} seed={`${tenantId}/${stack}`} size={64} />
        </div>
        <div className="grid min-w-0 gap-2">
          {view ? <StatusPill status={view.led} players={null} gone={false} word={view.word} /> : <p className="text-ink-dim">{t('common.loading')}</p>}
          {detail.data?.version && <p className="text-sm text-ink-dim">{t('manage.version', { version: detail.data.version })}</p>}
          {failed && (
            <p className="manage-cause">
              <WarningDiamond className="h-4 w-4 flex-none text-coral" aria-hidden />
              {cause ? causeLine(t, cause) : t('manage.cause.unknown', { subjectPart: '' })}
            </p>
          )}
        </div>
        <div className="manage-actions">
          {(state === 'stopped' || state === 'asleep') && (
            <Button
              variant="primary"
              size="lg"
              aria-disabled={pending !== null || undefined}
              onClick={() => run('start')}
            >
              {pending === 'start' ? busyWord('start') : failed ? t('manage.action.startAgain') : t('manage.action.start')}
            </Button>
          )}
          {state === 'running' && (
            <>
              <Button size="lg" aria-disabled={pending !== null || undefined} onClick={() => !pending && setAsking('restart')}>
                {pending === 'restart' ? busyWord('restart') : t('manage.action.restart')}
              </Button>
              <Button variant="danger" size="lg" aria-disabled={pending !== null || undefined} onClick={() => !pending && setAsking('stop')}>
                {pending === 'stop' ? busyWord('stop') : t('manage.action.stop')}
              </Button>
            </>
          )}
        </div>
      </div>
      {detail.data?.pendingRestart && <p className="note-plate">{t('manage.pendingRestart')}</p>}
      {action.isError && <ErrorCard error={action.error} />}
      <div className="manage-head-foot">
        <div className="grid min-w-0 flex-1 gap-1.5">
          <p className="text-sm font-semibold" aria-hidden>
            {t('manage.address')}
          </p>
          {detail.data?.address ? (
            <CopyRow text={detail.data.address} label={t('manage.address')} copy={t('server.copyAddress')} copied={t('server.addressCopied')} />
          ) : (
            <p className="text-sm text-ink-dim">{detail.data ? t('manage.noAddress') : ' '}</p>
          )}
        </div>
        <Button icon={<ExternalLink />} onClick={() => panelOnce((done) => void openManagePanel(tenantId, stack).finally(done))}>
          {t('manage.morePerland')}
        </Button>
      </div>
      {asking && (
        <Dialog
          open
          onClose={() => setAsking(null)}
          title={t(asking === 'stop' ? 'manage.action.stopTitle' : 'manage.action.restartTitle', { name: stack })}
          actions={
            <>
              <Button onClick={() => setAsking(null)}>{t('common.cancel')}</Button>
              <Button variant="danger" onClick={() => run(asking)}>
                {t(asking === 'stop' ? 'manage.action.stop' : 'manage.action.restart')}
              </Button>
            </>
          }
        >
          <p>{t(asking === 'stop' ? 'manage.action.stopBody' : 'manage.action.restartBody')}</p>
        </Dialog>
      )}
    </section>
  );
}
