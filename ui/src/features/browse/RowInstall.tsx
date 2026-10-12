import { useTranslation } from 'react-i18next';
import type { SearchHit } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { InstallDialog } from './InstallDialog';
import { useInstallFlow } from './useInstallFlow';

/** Install from a scoped result row: newest fitting version, same confirm rules. Never lit. */
export function RowInstall({ hit, instanceId }: { hit: SearchHit; instanceId: string }) {
  const { t } = useTranslation();
  const flow = useInstallFlow();
  return (
    <>
      <Button
        disabled={flow.busy}
        onClick={() => void flow.start({ project: hit.projectId, version: null, target: { kind: 'instance', id: instanceId } })}
      >
        {flow.busy ? t('project.installing') : t('browse.install')}
      </Button>
      {flow.confirming && (
        <InstallDialog view={flow.confirming} busy={flow.busy} onConfirm={() => void flow.install(flow.confirming!)} onClose={flow.cancel} />
      )}
    </>
  );
}
