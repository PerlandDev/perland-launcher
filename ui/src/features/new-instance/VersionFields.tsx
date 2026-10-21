import { useTranslation } from 'react-i18next';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Select } from '../../ui/Select';
import { Toggle } from '../../ui/Toggle';
import { loaderName } from '../home/labels';
import type { LoaderChoice, VersionChoice } from './useVersionChoice';

const LOADERS: LoaderChoice[] = ['none', 'fabric', 'quilt', 'forge', 'neoforge'];
const LOADER_KEY = {
  none: 'newInstance.loaderNone',
  fabric: 'newInstance.loaderFabric',
  quilt: 'newInstance.loaderQuilt',
  forge: 'newInstance.loaderForge',
  neoforge: 'newInstance.loaderNeoForge',
} as const;

/** The Minecraft version select and its Show snapshots toggle: what `VersionFields` shows first, and what a form with its own
 *  version list (a team server's, from the Engine) reuses. */
export interface MinecraftChoice {
  list: { id: string }[];
  mc: string | null;
  setMinecraft: (next: string) => void;
  snapshots: boolean;
  setSnapshots: (on: boolean) => void;
  versions: { isError: boolean; error: unknown; refetch: () => unknown };
}

export function MinecraftVersionField({ choice, error }: { choice: MinecraftChoice; error?: string | null }) {
  const { t } = useTranslation();
  const options = choice.list.map((v) => ({ value: v.id, label: v.id }));
  if (choice.mc && !options.some((o) => o.value === choice.mc)) options.unshift({ value: choice.mc, label: choice.mc });
  return (
    <div className="grid gap-3">
      <Select label={t('newInstance.version')} value={choice.mc ?? ''} onChange={choice.setMinecraft} options={options} disabled={options.length === 0} error={error} />
      <div className="flex items-center gap-3">
        <Toggle checked={choice.snapshots} onChange={choice.setSnapshots} label={t('newInstance.showSnapshots')} />
        <span className="text-sm text-ink-dim" aria-hidden>
          {t('newInstance.showSnapshots')}
        </span>
      </div>
      {choice.versions.isError && (
        <ErrorCard error={choice.versions.error} actions={<Button onClick={() => void choice.versions.refetch()}>{t('common.retry')}</Button>} />
      )}
    </div>
  );
}

export function VersionFields({
  choice,
  withLoader = true,
  error,
}: {
  choice: VersionChoice;
  withLoader?: boolean;
  /** A refusal of the Minecraft version, under its select. */
  error?: string | null;
}) {
  const { t } = useTranslation();
  const retry = (onClick: () => void) => <Button onClick={onClick}>{t('common.retry')}</Button>;
  return (
    <div className="grid gap-6">
      <MinecraftVersionField choice={choice} error={error} />
      {withLoader && (
        <fieldset className="grid gap-3">
          <legend className="mb-1.5 text-sm font-semibold">{t('newInstance.loader')}</legend>
          <div className="flex flex-wrap gap-2">
            {LOADERS.map((kind) => (
              <label key={kind} className="seg" data-checked={choice.loader === kind ? '' : undefined}>
                <input type="radio" name="loader" className="sr-only" value={kind} checked={choice.loader === kind} onChange={() => choice.setLoader(kind)} />
                {t(LOADER_KEY[kind])}
              </label>
            ))}
          </div>
          {choice.loader !== 'none' &&
            (choice.loaders.isSuccess && choice.loaderList.length === 0 ? (
              <p className="text-ink-dim">{t('newInstance.noLoaderForVersion', { loader: loaderName(choice.loader) })}</p>
            ) : (
              <Select
                label={t('newInstance.loaderVersion')}
                value={choice.chosenLoader ?? ''}
                onChange={choice.setLoaderVersion}
                disabled={choice.loaderList.length === 0 && !choice.chosenLoader}
                options={[
                  ...(choice.chosenLoader && !choice.loaderList.some((v) => v.version === choice.chosenLoader)
                    ? [{ value: choice.chosenLoader, label: choice.chosenLoader }]
                    : []),
                  ...choice.loaderList.map((v) => ({
                    value: v.version,
                    label: t(v.stable ? 'newInstance.stable' : 'newInstance.beta', { version: v.version }),
                  })),
                ]}
              />
            ))}
          {choice.loaders.isError && <ErrorCard error={choice.loaders.error} actions={retry(() => void choice.loaders.refetch())} />}
        </fieldset>
      )}
    </div>
  );
}
