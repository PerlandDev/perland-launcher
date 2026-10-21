import { downloadDir, join } from '@tauri-apps/api/path';
import { save } from '@tauri-apps/plugin-dialog';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { CategoryCount, Instance, ShareCategory, ShareChoice } from '../../ipc/schema';
import { useRun } from '../../state/runStore';
import { pushError, pushNotice } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Toggle } from '../../ui/Toggle';
import { formatBytes } from '../../ui/format';
import { useSingleFlight } from '../../ui/useSingleFlight';

/** Worlds are big and personal, so they start off; everything else starts on. */
const START: ShareChoice = { mods: true, settings: true, resourcePacks: true, shaderPacks: true, worlds: false };

/** "About 48 MB.": what the file carries itself, for the choices that are on. */
function carriedBytes(counts: CategoryCount[], choice: ShareChoice): number {
  return counts.filter((c) => choice[c.category]).reduce((sum, c) => sum + c.embeddedBytes, 0);
}

/** Share… (spec 2026-10-21): pick what goes in, hear what happens to the mods, save a .mrpack. Mount it only while open. */
export function ShareDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  // Asked fresh each time the dialog opens: the folder may have changed since.
  const preview = useQuery({
    queryKey: ['share', 'preview', instance.id],
    queryFn: () => commands.sharePreview(instance.id),
    gcTime: 0,
    retry: false,
  });
  const [choice, setChoice] = useState<ShareChoice>(START);
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<unknown>(null);
  const once = useSingleFlight();
  const running = useRun(instance.id).state !== 'idle';
  const counts = preview.data?.counts ?? [];
  const nothingOn = counts.length > 0 && !counts.some((c) => choice[c.category]);
  const blocked = saving || nothingOn || running;

  const detail = (c: CategoryCount): string => {
    const size = formatBytes(c.embeddedBytes, i18n.language);
    switch (c.category) {
      case 'mods':
        return t('share.modsCount', { count: c.files });
      case 'settings':
        return t('share.settingsDetail');
      case 'resourcePacks':
        return t('share.resourcePacksCount', { count: c.files });
      case 'shaderPacks':
        return t('share.shaderPacksCount', { count: c.files });
      case 'worlds':
        return t('share.worldsCount', { count: c.files, size });
    }
  };
  const label: Record<ShareCategory, string> = {
    mods: t('share.mods'),
    settings: t('share.settings'),
    resourcePacks: t('share.resourcePacks'),
    shaderPacks: t('share.shaderPacks'),
    worlds: t('share.worlds'),
  };

  const mods = counts.find((c) => c.category === 'mods');
  const modsLine = (() => {
    if (!mods || !choice.mods) return null;
    const embedded = mods.files - mods.linked;
    if (embedded === 0) return t('share.modsAll');
    if (mods.linked === 0) return t('share.modsNone', { count: embedded });
    return `${t('share.modsLinked', { count: mods.linked })} ${t('share.modsEmbedded', { count: embedded })}`;
  })();

  const saveFile = async () => {
    if (!preview.data) return;
    const name = preview.data.fileName;
    const defaultPath = await downloadDir()
      .then((dir) => join(dir, name))
      .catch(() => name);
    const path = await save({ defaultPath, filters: [{ name: '.mrpack', extensions: ['mrpack'] }] });
    if (!path) return;
    setFailure(null);
    setSaving(true);
    try {
      await commands.shareExport(instance.id, choice, path);
    } catch (error) {
      setFailure(error);
      return;
    } finally {
      setSaving(false);
    }
    onClose();
    pushNotice({
      title: t('share.saved'),
      action: { label: t('share.show'), run: () => void commands.shareReveal().catch(pushError) },
    });
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={t('share.title', { name: instance.name })}
      actions={
        <>
          {/* The size, or why Save is off, rides the action row so it stays in view while the list scrolls. */}
          {preview.isSuccess && (
            <p className="mr-auto min-w-0 flex-1 self-center text-sm text-ink" aria-live="polite">
              {nothingOn || running ? (
                <span id="share-blocked" className="font-semibold">
                  {running ? t('share.running') : t('share.pickSomething')}
                </span>
              ) : (
                <span className="tabular">{t('share.size', { size: formatBytes(carriedBytes(counts, choice), i18n.language) })}</span>
              )}
            </p>
          )}
          <Button variant="ghost" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          {preview.isSuccess && (
            <Button
              variant="primary"
              aria-disabled={blocked || undefined}
              aria-describedby={nothingOn || running ? 'share-blocked' : undefined}
              onClick={() => {
                if (!blocked) once((done) => void saveFile().catch(pushError).finally(done));
              }}
            >
              {saving ? t('share.saving') : t('share.save')}
            </Button>
          )}
        </>
      }
    >
      <div className="share-sheet grid gap-4">
        <p>{t('share.intro')}</p>
        {preview.isPending && <p role="status">{t('share.loading')}</p>}
        {preview.isError && (
          <ErrorCard
            error={preview.error}
            actions={<Button onClick={() => void preview.refetch()}>{t('common.retry')}</Button>}
          />
        )}
        {preview.isSuccess &&
          (counts.length === 0 ? (
            <p>{t('share.versionOnly')}</p>
          ) : (
            <section aria-labelledby="share-what" className="grid gap-1">
              <h3 id="share-what" className="text-sm font-semibold text-ink">
                {t('share.whatGoesIn')}
              </h3>
              <ul>
                {counts.map((c) => (
                  <li key={c.category} className="content-row">
                    <div className="min-w-0 flex-1">
                      <p className="font-semibold text-ink">{label[c.category]}</p>
                      <p className="tabular text-sm text-ink-dim">{detail(c)}</p>
                      {c.category === 'mods' && modsLine && (
                        <p className="text-sm text-ink" aria-live="polite">
                          {modsLine}
                        </p>
                      )}
                    </div>
                    <Toggle
                      checked={choice[c.category]}
                      label={label[c.category]}
                      disabled={saving}
                      onChange={(on) => setChoice((now) => ({ ...now, [c.category]: on }))}
                    />
                  </li>
                ))}
              </ul>
            </section>
          ))}
        {failure !== null && <ErrorCard error={failure} />}
      </div>
    </Dialog>
  );
}
