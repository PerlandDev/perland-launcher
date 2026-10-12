import { useTranslation } from 'react-i18next';
import type { InstallPlanView } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { formatBytes } from '../../ui/format';
import { gameLabel } from './labels';

export function InstallDialog({
  view,
  busy,
  onConfirm,
  onClose,
}: {
  view: InstallPlanView;
  busy: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const name = view.items[0]?.title ?? '';
  const title =
    view.target.kind === 'new'
      ? t('install.titleNew', { name, game: gameLabel(view.target.minecraft, view.target.loader) })
      : t('install.titleExisting', { name, instance: view.target.name });
  return (
    <Dialog
      open
      onClose={busy ? () => {} : onClose}
      title={title}
      actions={
        <>
          <Button disabled={busy} onClick={onClose}>{t('common.cancel')}</Button>
          <Button variant="primary" disabled={busy} onClick={onConfirm}>
            {busy ? t('project.installing') : t('install.confirm')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <ul className="install-items">
          {view.items.map((item) => (
            <li key={item.projectId}>
              {item.reason.kind === 'requested' ? (
                <strong>{item.title}</strong>
              ) : (
                <>
                  {`+ ${item.title} · `}
                  <span className="text-ink-dim">
                    {item.reason.kind === 'dependency' ? t('install.neededBy', { name: item.reason.of }) : t('install.forShaders')}
                  </span>
                </>
              )}
            </li>
          ))}
        </ul>
        {view.skipped.length > 0 && (
          <p className="text-ink-dim">
            {t('install.alreadyHave', {
              names: view.skipped.map((s) => (s.reason === 'in_pack' ? t('install.inPack', { name: s.title }) : s.title)).join(', '),
            })}
          </p>
        )}
        <p className="tabular text-sm text-ink-dim">{t('install.total', { size: formatBytes(view.totalSize, i18n.language) })}</p>
      </div>
    </Dialog>
  );
}
