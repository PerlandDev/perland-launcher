import { downloadDir, join } from '@tauri-apps/api/path';
import { save } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { pushError, pushNotice } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Download } from '../../ui/Icon';
import { useSingleFlight } from '../../ui/useSingleFlight';

/** The player's local date (YYYY-MM-DD): an evening export in the Americas is not named tomorrow. */
const today = () => {
  const now = new Date();
  const two = (n: number) => String(n).padStart(2, '0');
  return `${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())}`;
};

/** Settings → Diagnostics (spec §5.2): a zip for whoever helps the player; nothing uploads. */
export function Diagnostics() {
  const { t } = useTranslation();
  const once = useSingleFlight();
  const exportZip = async () => {
    const name = `perland-diagnostics-${today()}.zip`;
    const defaultPath = await downloadDir()
      .then((dir) => join(dir, name))
      .catch(() => name);
    const path = await save({ defaultPath, filters: [{ name: 'Zip', extensions: ['zip'] }] });
    if (!path) return;
    await commands.diagnosticsExport(path);
    pushNotice({
      title: t('settings.diagnostics.saved'),
      action: { label: t('settings.diagnostics.show'), run: () => void commands.diagnosticsReveal().catch(pushError) },
    });
  };
  return (
    <section className="grid gap-4" aria-labelledby="diagnostics-heading">
      <h2 id="diagnostics-heading" className="text-lg font-bold">
        {t('settings.diagnostics.title')}
      </h2>
      <p className="text-sm text-ink-dim">
        {t('settings.diagnostics.body')}
      </p>
      <div>
        <Button icon={<Download />} onClick={() => once((done) => void exportZip().catch(pushError).finally(done))}>
          {t('settings.diagnostics.export')}
        </Button>
      </div>
    </section>
  );
}
