import { useTranslation } from 'react-i18next';
import type { ModSourceCounts, TrustPrompt } from '../../ipc/schema';
import { InfoBox, Upload } from '../../ui/Icon';
import { SwitchRow } from '../../ui/SwitchRow';

type SourceKey = keyof ModSourceCounts;

/** The rows of "Where they come from", in this order. Uploaded files are called out (their owner put them there, and no
 *  catalogue published them); the rest read as plain counts. Seam: once the Engine's mod-safety scan is live, its
 *  per-verdict counts and a "Perland removed N files" line join these rows; nothing reads or invents them today. */
const ROWS: { key: SourceKey; label: string; callout?: boolean }[] = [
  { key: 'modrinth', label: 'trust.fromModrinth' },
  { key: 'curseforge', label: 'trust.fromCurseforge' },
  { key: 'web', label: 'trust.fromWeb' },
  { key: 'uploaded', label: 'trust.fromUploaded', callout: true },
  { key: 'other', label: 'trust.fromOther' },
];

/** What the trust step says, wherever it shows (Join server's screen, the dialog): who runs the server, what joining
 *  installs and where it comes from, and that mods are programs. Facts only, in the server's own numbers. */
export function TrustFacts({ prompt }: { prompt: TrustPrompt }) {
  const { t } = useTranslation();
  const installs = prompt.modrinthPack
    ? t('trust.modrinthPack')
    : prompt.pack
      ? t('trust.installsNamed', { pack: prompt.pack, count: prompt.modCount })
      : t('trust.installs', { count: prompt.modCount });
  const sources = prompt.sources;
  const rows = sources ? ROWS.filter((row) => sources[row.key] > 0) : [];
  return (
    <div className="trust-facts grid gap-4 text-ink">
      <div className="grid gap-1">
        {prompt.team && <p className="font-semibold">{t('trust.runBy', { team: prompt.team })}</p>}
        <p>{installs}</p>
      </div>
      {sources && rows.length > 0 && (
        <section className="well grid gap-2 p-3" aria-labelledby="trust-sources">
          <h3 id="trust-sources" className="text-sm font-semibold text-ink-dim">
            {t('trust.sourcesHeading')}
          </h3>
          <dl className="grid gap-1.5 text-sm">
            {rows.map((row) => (
              <div key={row.key} className="flex items-center justify-between gap-6" data-callout={row.callout ? '' : undefined}>
                <dt className={row.callout ? 'flex items-center gap-1.5 font-semibold' : undefined}>
                  {row.callout && <Upload className="h-4 w-4 flex-none" aria-hidden />}
                  {t(row.label)}
                </dt>
                <dd className="tabular font-semibold">{sources[row.key]}</dd>
              </div>
            ))}
          </dl>
        </section>
      )}
      {!sources && !prompt.modrinthPack && <p className="text-sm text-ink-dim">{t('trust.sourcesUnknown')}</p>}
      <p className="flex items-start gap-2 text-blue-ink">
        <InfoBox className="mt-0.5 h-5 w-5 flex-none" aria-hidden />
        <span>{t('trust.programs')}</span>
      </p>
    </div>
  );
}

/** "Update this server's pack automatically": the step's one choice, also in Instance settings. */
export function AutoUpdateSwitch({ checked, disabled, onChange }: { checked: boolean; disabled?: boolean; onChange: (next: boolean) => void }) {
  const { t } = useTranslation();
  return <SwitchRow label={t('trust.autoUpdate')} hint={t('trust.autoUpdateHint')} checked={checked} disabled={disabled} onChange={onChange} />;
}
