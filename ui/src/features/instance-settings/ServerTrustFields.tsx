import { useTranslation } from 'react-i18next';
import type { Instance } from '../../ipc/schema';
import { useInstanceTrust, useInstanceTrustForget } from '../../state/queries';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { AutoUpdateSwitch } from '../trust/TrustFacts';

/** Instance settings, Server: whether the player trusts the server, the way to take it back, and whether its pack updates on
 *  its own (saved with the screen's Save, like the rest of the form). Taking trust back is immediate and installs nothing;
 *  the next Play shows the trust step again. */
export function ServerTrustFields({
  instance,
  running,
  autoUpdate,
  onAutoUpdate,
}: {
  instance: Instance;
  running: boolean;
  autoUpdate: boolean;
  onAutoUpdate: (next: boolean) => void;
}) {
  const { t } = useTranslation();
  const trusted = instance.serverTrust !== undefined;
  // Untrusted: ask whether the server needs the step at all (a vanilla server doesn't).
  const step = useInstanceTrust(instance.id, !trusted);
  const forget = useInstanceTrustForget(instance.id, { inline: true });
  const needsStep = !trusted && step.data != null;
  return (
    <div className="mt-5 grid gap-4" role="group" aria-labelledby="trust-settings">
      <div className="grid gap-1">
        <h3 id="trust-settings" className="font-semibold">
          {t('trust.settingsHeading')}
        </h3>
        {trusted && <p>{t('trust.trusted')}</p>}
        {needsStep && <p>{forget.isSuccess ? t('trust.stopped') : t('trust.untrusted')}</p>}
      </div>
      {trusted && (
        <div className="grid gap-2">
          <div>
            <Button disabled={running || forget.isPending} aria-describedby={running ? 'trust-running' : undefined} onClick={() => forget.mutate()}>
              {t('trust.stop')}
            </Button>
          </div>
          {running && (
            <p id="trust-running" className="text-sm text-ink-dim">
              {t('trust.running')}
            </p>
          )}
        </div>
      )}
      {forget.isError && <ErrorCard error={forget.error} />}
      <AutoUpdateSwitch checked={autoUpdate} onChange={onAutoUpdate} />
    </div>
  );
}
