import { useQuery } from '@tanstack/react-query';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance, ManualFile } from '../../ipc/schema';
import { qk } from '../../state/queries';
import { runStore } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { Check, ExternalLink } from '../../ui/Icon';
import { useSingleFlight } from '../../ui/useSingleFlight';

/** Files whose authors block other launchers: open each page, and the launcher picks the
 *  downloads up from the Downloads folder (polled). Continue plays again once all are found. */
export function ManualDownloads({ instance, files, pollMs = 2000 }: { instance: Instance; files: ManualFile[]; pollMs?: number }) {
  const { t } = useTranslation();
  const progress = useQuery({
    queryKey: qk.manual(instance.id),
    queryFn: () => commands.manualImport(instance.id),
    refetchInterval: (query) => (query.state.data && query.state.data.total > 0 && query.state.data.remaining.length === 0 ? false : pollMs),
    staleTime: 0,
  });
  // Total 0 means the launcher no longer knows the list (it restarted): trust the run's list.
  const known = progress.data !== undefined && progress.data.total > 0;
  const remaining = known ? progress.data!.remaining : files;
  const total = known ? progress.data!.total : files.length;
  const found = total - remaining.length;
  const close = () => runStore.dismissExit(instance.id);
  const playOnce = useSingleFlight();
  const play = () => playOnce((done) => void commands.play(instance.id).then(close, pushError).finally(done));
  return (
    <Dialog
      open
      onClose={close}
      title={t('manual.title')}
      actions={
        <>
          <Button onClick={() => void commands.openFolder({ kind: 'downloads' }).catch(pushError)}>{t('manual.openDownloads')}</Button>
          <Button variant="primary" disabled={remaining.length > 0} onClick={play}>
            {t('common.continue')}
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        <p>{t('manual.body')}</p>
        <p role="status" className="tabular font-semibold text-ink">
          {t('manual.found', { found, total })}
        </p>
        <ul className="grid gap-2">
          {files.map((file) => {
            const got = !remaining.some((r) => r.path === file.path);
            return (
              <li key={file.path} className="well flex items-center gap-4 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <p className="truncate font-semibold text-ink">{file.name}</p>
                  <p className="selectable truncate font-mono text-sm">{file.fileName}</p>
                </div>
                {got ? (
                  <span className="flex items-center gap-2 text-lime">
                    <Check className="h-5 w-5" aria-hidden />
                    {t('manual.got')}
                  </span>
                ) : (
                  <Button icon={<ExternalLink />} onClick={() => void openUrl(file.pageUrl).catch(pushError)}>
                    {t('manual.openPage')}
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      </div>
    </Dialog>
  );
}
