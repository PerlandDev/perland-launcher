import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { taskLabel } from '../../ipc/errors';
import type { ProjectDetail } from '../../ipc/schema';
import { useContentProgress } from '../../state/contentProgress';
import { useAddPack, useBrowseInstalled, useBrowseVersions, useInstances } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Meter } from '../../ui/Meter';
import { Select } from '../../ui/Select';
import type { BrowseRoute } from './Browse';
import { fitFor, fitsNewInstance, fitsVersion, pickDefault } from './compat';
import { InstallDialog } from './InstallDialog';
import { versionLabel } from './labels';
import { useInstallFlow } from './useInstallFlow';

const NEW = '__new__';

function ModpackPlate({ project }: { project: ProjectDetail }) {
  const { t } = useTranslation();
  const versions = useBrowseVersions(project.projectId);
  const add = useAddPack();
  const [chosen, setChosen] = useState<string | null>(null);
  const list = versions.data ?? [];
  const version = chosen ?? pickDefault(list)?.id ?? '';
  return (
    <div className="panel action-plate grid gap-4 p-5">
      {versions.isError ? (
        <ErrorCard error={versions.error} actions={<Button onClick={() => void versions.refetch()}>{t('common.retry')}</Button>} />
      ) : (
        <Select
          label={t('project.version')}
          value={version}
          onChange={setChosen}
          disabled={list.length === 0}
          options={list.map((v) => ({ value: v.id, label: versionLabel(v) }))}
        />
      )}
      <Button
        variant="primary"
        size="lg"
        disabled={!versions.isSuccess || !version || add.isPending}
        onClick={() =>
          add.mutate(
            { kind: 'link', url: `modrinth:${project.projectId}@${version}` },
            { onSuccess: (instance) => navigate({ name: 'home', id: instance.id }) },
          )
        }
      >
        {add.isPending ? t('project.creating') : t('project.createInstance')}
      </Button>
    </div>
  );
}

function ContentPlate({ project, route }: { project: ProjectDetail; route: BrowseRoute }) {
  const { t, i18n } = useTranslation();
  const targetId = useId();
  const versions = useBrowseVersions(project.projectId);
  const instances = useInstances();
  const all = versions.data ?? [];
  const list = instances.data?.instances ?? [];
  const fits = new Map(list.map((i) => [i.id, fitFor(all, project.kind, i)] as const));
  const firstFit = list.find((i) => fits.get(i.id)?.ok)?.id;
  const [picked, setPicked] = useState<string | null>(null);
  const [chosen, setChosen] = useState<string | null>(null);
  // The target is always something the picker can show: a pick or the route's instance only while it is in the list.
  const known = (id: string | null) => (id !== null && list.some((i) => i.id === id) ? id : null);
  const target = (picked === NEW ? NEW : known(picked)) ?? known(route.instance ?? null) ?? firstFit ?? NEW;
  const instance = list.find((i) => i.id === target) ?? null;
  // A new instance is made from the version itself, so only versions the core can make one from are offered.
  const options = instance ? all.filter((v) => fitsVersion(v, project.kind, instance)) : all.filter((v) => fitsNewInstance(v, project.kind));
  const version = options.some((v) => v.id === chosen) ? chosen : (pickDefault(options)?.id ?? null);
  const installed = useBrowseInstalled(instance?.id ?? null);
  const already = installed.data?.includes(project.projectId) ?? false;
  const flow = useInstallFlow();
  const progress = useContentProgress(instance?.id ?? null);
  const reason = (id: string) => {
    const fit = fits.get(id);
    if (!versions.isSuccess || !fit || fit.ok) return null;
    return t(fit.reason === 'needsLoader' ? 'project.needsLoader' : 'project.noVersion');
  };
  return (
    <div className="panel action-plate grid gap-4 p-5">
      <div className="grid gap-1.5">
        <label htmlFor={targetId} className="text-sm font-semibold">
          {t('project.installTo')}
        </label>
        <select
          id={targetId}
          className="field-input"
          value={target}
          onChange={(e) => {
            setPicked(e.target.value);
            setChosen(null);
          }}
        >
          {list.map((i) => {
            const why = reason(i.id);
            return (
              <option key={i.id} value={i.id} disabled={why !== null}>
                {why ? `${i.name} — ${why}` : i.name}
              </option>
            );
          })}
          <option value={NEW}>{t('project.newInstance')}</option>
        </select>
      </div>
      {versions.isError ? (
        <ErrorCard error={versions.error} actions={<Button onClick={() => void versions.refetch()}>{t('common.retry')}</Button>} />
      ) : options.length > 0 ? (
        <Select
          label={t('project.version')}
          value={version ?? ''}
          onChange={setChosen}
          options={options.map((v) => ({ value: v.id, label: versionLabel(v) }))}
        />
      ) : (
        versions.isSuccess && <p className="text-ink-dim">{t('project.noVersions')}</p>
      )}
      {already && instance ? (
        <p className="installed-lamp">{t('project.alreadyThere', { name: instance.name })}</p>
      ) : (
        <Button
          variant="primary"
          size="lg"
          disabled={flow.busy || version === null || !instances.isSuccess || !versions.isSuccess}
          onClick={() =>
            void flow.start({
              project: project.projectId,
              version,
              target: instance ? { kind: 'instance', id: instance.id } : { kind: 'new' },
            })
          }
        >
          {flow.busy ? t('project.installing') : t('project.install')}
        </Button>
      )}
      {flow.busy && progress && (
        <Meter value={progress.total > 0 ? progress.done / progress.total : 0} label={taskLabel(i18n, progress.task)} />
      )}
      {flow.confirming && (
        <InstallDialog
          view={flow.confirming}
          busy={flow.busy}
          onConfirm={() => void flow.install(flow.confirming!)}
          onClose={flow.cancel}
        />
      )}
    </div>
  );
}

export function ActionPlate({ project, route }: { project: ProjectDetail; route: BrowseRoute }) {
  return project.kind === 'modpack' ? <ModpackPlate project={project} /> : <ContentPlate project={project} route={route} />;
}
