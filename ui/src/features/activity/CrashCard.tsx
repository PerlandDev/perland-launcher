import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { describeError, supportText } from '../../ipc/errors';
import type { ExitInfo, Instance } from '../../ipc/schema';
import { runStore } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Close, Copy, Undo, WarningDiamond } from '../../ui/Icon';
import { GoBackDialog } from '../home/GoBackDialog';
import { versionLine } from '../home/labels';
import { useRestorePoint } from '../home/useRestorePoint';
import { CrashFixes } from './CrashFixes';
import { useModrinthMods } from './useModrinthMods';

/** Why the game ended, in plain words, with the fixes the launcher can apply itself. */
export function CrashCard({ instance, exit, lines }: { instance: Instance; exit: ExitInfo; lines: string[] }) {
  const { t, i18n } = useTranslation();
  const [showDetails, setShowDetails] = useState(false);
  const point = useRestorePoint(instance.id).data ?? null;
  const [confirming, setConfirming] = useState(false);
  const found = useModrinthMods(exit.fixes);
  // A missing mod Modrinth knows is named as its Find button names it ("Fabric API", not "fabric-api").
  const missing = exit.fixes.map((fix) => (fix.kind === 'find_mod' ? found.get(fix.project)?.title : undefined)).find(Boolean);
  const cause = exit.cause ? describeError(i18n, missing ? { ...exit.cause, params: { ...exit.cause.params, missing } } : exit.cause) : null;
  const copy = () =>
    navigator.clipboard
      .writeText(
        supportText([
          instance.name,
          versionLine(t, instance),
          `exit code: ${exit.code ?? 'none'}`,
          exit.error ? `error: ${exit.error.code}: ${exit.error.message}` : '',
          exit.error?.detail ?? '',
          exit.cause ? `cause: ${exit.cause.code}: ${exit.cause.message}` : '',
          exit.cause?.detail ?? '',
          '',
          ...lines.slice(-200),
        ]),
      )
      .catch(pushError);
  const anyFix = exit.fixes.some((fix) => fix.kind !== 'find_mod' || found.has(fix.project));
  // A crash right after an update offers going back: after the cause's own fixes, the primary when it's the only one.
  const goBack = exit.restore && point?.kind === 'beforeChange';
  // Without a fix to show, Copy for support is the card's one action, raised as it always was.
  const copyButton = (
    <Button variant={anyFix || goBack ? 'ghost' : 'secondary'} icon={<Copy />} onClick={copy}>
      {t('activity.copyForSupport')}
    </Button>
  );
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

  return (
    <div role="alert" className="panel grid gap-3 p-5">
      <div className="flex items-start gap-3">
        <WarningDiamond className="mt-0.5 h-6 w-6 flex-none text-coral" aria-hidden />
        <div className="grid flex-1 gap-1">
          <h3 className="font-bold">{cause ? cause.message : t('activity.crashTitle', { name: instance.name })}</h3>
          <p className="text-ink-dim">{cause?.fix ?? t('activity.crashCode', { code: exit.code ?? '?' })}</p>
        </div>
        {dismiss}
      </div>
      <CrashFixes instance={instance} fixes={exit.fixes} found={found}>
        {goBack && (
          <Button variant={anyFix ? 'ghost' : 'secondary'} icon={<Undo />} onClick={() => setConfirming(true)}>
            {t('restore.menu')}
          </Button>
        )}
        {copyButton}
        {cause?.detail && (
          <Button variant="ghost" aria-expanded={showDetails} onClick={() => setShowDetails((v) => !v)}>
            {showDetails ? t('common.hideDetails') : t('common.details')}
          </Button>
        )}
      </CrashFixes>
      {showDetails && cause?.detail && (
        <pre className="selectable max-h-48 overflow-auto rounded bg-ground p-3 font-mono text-sm whitespace-pre-wrap text-ink-dim">
          {cause.detail}
        </pre>
      )}
      {confirming && point && <GoBackDialog instance={instance} point={point} onClose={() => setConfirming(false)} />}
    </div>
  );
}
