import { useTranslation } from 'react-i18next';
import { taskLabel } from '../../ipc/errors';
import type { Instance } from '../../ipc/schema';
import { closeActivity, openActivity, useActivity } from '../../state/activity';
import { isCrash } from '../../state/exit';
import { useInstances } from '../../state/queries';
import { useActiveIds, useLog, useRun } from '../../state/runStore';
import { formatBytes, formatCount } from '../../ui/format';
import { ChevronDown } from '../../ui/Icon';
import { Meter } from '../../ui/Meter';
import { CrashCard } from './CrashCard';
import { LogView } from './LogView';

function InstanceActivity({ instance }: { instance: Instance }) {
  const { t, i18n } = useTranslation();
  const run = useRun(instance.id);
  const lines = useLog(instance.id);
  const exit = run.lastExit;
  const showExit = exit && (exit.error || isCrash(exit));
  const progress = run.progress;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 p-5">
      {showExit && <CrashCard instance={instance} exit={exit} lines={lines} />}
      {progress && (run.state === 'queued' || run.state === 'preparing') && (
        <div className="grid gap-2">
          <div className="flex items-baseline justify-between gap-4">
            <span className="font-semibold">{taskLabel(i18n, progress.task)}</span>
            <span className="tabular text-sm text-ink-dim">
              {t('play.files', { done: formatCount(progress.done, i18n.language), total: formatCount(progress.total, i18n.language) })}
              {progress.bytes > 0 && ` · ${formatBytes(progress.bytes, i18n.language)}`}
            </span>
          </div>
          <Meter value={progress.total > 0 ? progress.done / progress.total : 0} label={taskLabel(i18n, progress.task)} />
        </div>
      )}
      <LogView lines={lines} />
    </div>
  );
}

/** A drawer along the bottom: what's downloading, what the game prints, and why it stopped.
 *  Open, it is 18rem tall and the Shell pads `main` by the same amount, so Play stays in view. */
export function ActivityDrawer() {
  const { t } = useTranslation();
  const activity = useActivity();
  const active = useActiveIds();
  const listing = useInstances();
  const instances = listing.data?.instances ?? [];
  const focusId = activity.focus ?? active[0] ?? instances[0]?.id ?? null;
  const focus = instances.find((i) => i.id === focusId) ?? null;
  const tabs = instances.filter((i) => active.includes(i.id) || i.id === focusId);

  return (
    <div className={`absolute inset-x-0 bottom-0 z-20 flex flex-col bg-panel ${activity.open ? 'h-[18rem]' : ''}`}>
      <button
        type="button"
        className="flex h-9 flex-none items-center gap-3 px-5 text-sm font-semibold text-ink-dim hover:text-ink"
        aria-expanded={activity.open}
        onClick={() => (activity.open ? closeActivity() : openActivity())}
      >
        <ChevronDown className={`h-4 w-4 ${activity.open ? '' : 'rotate-180'}`} aria-hidden />
        {t('activity.title')}
        {active.length > 0 && <span className="badge text-lime">{active.length}</span>}
      </button>
      {activity.open && (
        <div className="flex min-h-0 flex-1 flex-col">
          {tabs.length > 1 && (
            <div role="group" aria-label={t('activity.instances')} className="flex gap-1 px-5">
              {tabs.map((i) => (
                <button
                  key={i.id}
                  type="button"
                  aria-pressed={i.id === focusId}
                  className="rounded-t px-3 py-1.5 text-sm aria-pressed:bg-ground"
                  onClick={() => openActivity(i.id)}
                >
                  {i.name}
                </button>
              ))}
            </div>
          )}
          {focus ? <InstanceActivity instance={focus} /> : <p className="p-5 text-ink-dim">{t('activity.empty')}</p>}
        </div>
      )}
    </div>
  );
}
