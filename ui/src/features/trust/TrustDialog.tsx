import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { isLauncherError } from '../../ipc/errors';
import type { TrustPrompt } from '../../ipc/schema';
import { qk } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';
import { closeTrust, useTrustRequest, type TrustRequest } from '../../state/trustPrompt';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { AutoUpdateSwitch, TrustFacts } from './TrustFacts';

/** The trust step as a dialog, drawn once by the app: before a server list's Play adds a server, and when a Play is refused
 *  because the player hasn't trusted the server yet. Trust and play is its one lit key; Not now installs nothing. */
export function TrustDialog() {
  const request = useTrustRequest();
  if (!request) return null;
  // Keyed, so a new ask starts with its own switch position.
  return request.kind === 'join' ? (
    <JoinTrust key={`join:${request.prompt.name}`} prompt={request.prompt} />
  ) : (
    <InstanceTrust key={`instance:${request.id}`} request={request} />
  );
}

function JoinTrust({ prompt }: { prompt: TrustPrompt }) {
  return <TrustStepDialog prompt={prompt} onTrust={(autoUpdate) => closeTrust(autoUpdate)} />;
}

/** A refused Play: read the step for the instance, record the answer, play again. A server that needs no step any more
 *  (the player owns it, or it dropped its modpack) just plays. */
function InstanceTrust({ request }: { request: Extract<TrustRequest, { kind: 'instance' }> }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { id, join } = request;
  const prompt = useQuery({ queryKey: ['instanceTrust', id], queryFn: () => commands.instanceTrust(id), staleTime: 0, gcTime: 0, retry: false });
  const play = () => playAgain(id, join);
  const settled = prompt.isError || prompt.data === null;
  useEffect(() => {
    if (prompt.isError) {
      closeTrust();
      pushError(prompt.error);
    } else if (prompt.data === null) {
      closeTrust();
      void playAgain(id, join);
    }
  }, [prompt.isError, prompt.error, prompt.data, id, join]);
  if (settled) return null;
  if (!prompt.data) {
    return (
      <Dialog open onClose={() => closeTrust()} title={t('trust.checking')}>
        <p aria-busy="true">{t('trust.checking')}</p>
      </Dialog>
    );
  }
  const trust = async (autoUpdate: boolean) => {
    try {
      await commands.instanceTrustSet(id, autoUpdate);
      await qc.invalidateQueries({ queryKey: qk.instances });
      closeTrust();
      await play();
    } catch (error) {
      closeTrust();
      pushError(error);
    }
  };
  return <TrustStepDialog prompt={prompt.data} onTrust={trust} />;
}

/** The Play the step stood in for, then the instance's Home (a server list's Play lands there too). A second refusal is
 *  never answered with the step again, so a server that keeps refusing can't loop it. */
async function playAgain(id: string, join: boolean) {
  try {
    await commands.play(id, join);
    navigate({ name: 'home', id });
  } catch (error) {
    if (!(isLauncherError(error) && error.code === 'server_trust_needed')) pushError(error);
  }
}

function TrustStepDialog({ prompt, onTrust }: { prompt: TrustPrompt; onTrust: (autoUpdate: boolean) => void | Promise<void> }) {
  const { t } = useTranslation();
  const [autoUpdate, setAutoUpdate] = useState(prompt.autoUpdate);
  const once = useSingleFlight();
  return (
    <Dialog
      open
      onClose={() => closeTrust()}
      title={t('trust.title', { name: prompt.name })}
      actions={
        <>
          <Button onClick={() => closeTrust()}>{t('trust.notNow')}</Button>
          <Button variant="primary" onClick={() => once((done) => void Promise.resolve(onTrust(autoUpdate)).finally(done))}>
            {t('trust.trustAndPlay')}
          </Button>
        </>
      }
    >
      <div className="grid gap-5">
        <TrustFacts prompt={prompt} />
        <AutoUpdateSwitch checked={autoUpdate} onChange={setAutoUpdate} />
      </div>
    </Dialog>
  );
}
