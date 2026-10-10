import { useTranslation } from 'react-i18next';
import { supportText } from '../../ipc/errors';
import type { ExitInfo, Instance } from '../../ipc/schema';
import { suggestMemory } from '../../state/exit';
import { useAppInfo, useUpdateInstance } from '../../state/queries';
import { runStore } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { formatGb } from '../../ui/format';
import { Close, Copy, MemoryStick, WarningDiamond } from '../../ui/Icon';
import { versionLine } from '../home/labels';

/** Why the game ended, in plain words, with the one fix the launcher can apply itself. */
export function CrashCard({ instance, exit, lines }: { instance: Instance; exit: ExitInfo; lines: string[] }) {
  const { t, i18n } = useTranslation();
  const appInfo = useAppInfo();
  const update = useUpdateInstance();
  const raised = appInfo.data ? suggestMemory(instance.java.memoryMb, appInfo.data.systemMemoryMb) : null;
  const copy = () =>
    navigator.clipboard
      .writeText(
        supportText([
          instance.name,
          versionLine(t, instance),
          `exit code: ${exit.code ?? 'none'}`,
          exit.error ? `error: ${exit.error.code}: ${exit.error.message}` : '',
          exit.error?.detail ?? '',
          '',
          ...lines.slice(-200),
        ]),
      )
      .catch(pushError);
  const dismiss = (
    <Button variant="ghost" size="icon" aria-label={t('common.dismiss')} icon={<Close />} onClick={() => runStore.dismissExit(instance.id)} />
  );

  if (exit.error) {
    return (
      <div className="grid gap-3">
        <div className="flex items-center justify-between gap-3">
          <h3 className="font-bold">{t('activity.failedTitle', { name: instance.name })}</h3>
          {dismiss}
        </div>
        <ErrorCard error={exit.error} actions={<Button icon={<Copy />} onClick={copy}>{t('activity.copyForSupport')}</Button>} />
      </div>
    );
  }

  const oom = exit.crash === 'out_of_memory';
  return (
    <div role="alert" className="panel grid gap-3 p-5">
      <div className="flex items-start gap-3">
        <WarningDiamond className="mt-0.5 h-6 w-6 flex-none text-coral" aria-hidden />
        <div className="grid flex-1 gap-1">
          <h3 className="font-bold">{oom ? t('activity.oomTitle') : t('activity.crashTitle', { name: instance.name })}</h3>
          <p className="text-ink-dim">{oom ? t('activity.oomBody') : t('activity.crashCode', { code: exit.code ?? '?' })}</p>
        </div>
        {dismiss}
      </div>
      <div className="flex flex-wrap gap-3">
        {oom && raised && (
          <Button
            variant="primary"
            icon={<MemoryStick />}
            onClick={() =>
              update.mutate(
                { ...instance, java: { ...instance.java, memoryMb: raised } },
                { onSuccess: () => runStore.dismissExit(instance.id) },
              )
            }
          >
            {t('activity.raiseMemory', { gb: formatGb(raised, i18n.language) })}
          </Button>
        )}
        <Button icon={<Copy />} onClick={copy}>
          {t('activity.copyForSupport')}
        </Button>
      </div>
    </div>
  );
}
