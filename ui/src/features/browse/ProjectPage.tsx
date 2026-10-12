import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { ProjectDetail } from '../../ipc/schema';
import { useBrowseProject } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { formatCompact } from '../../ui/format';
import { ExternalLink } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { ActionPlate } from './ActionPlate';
import type { BrowseRoute } from './Browse';
import { Gallery } from './Gallery';
import { categoryLabel } from './labels';
import { ProjectIcon } from './ProjectIcon';
import { ProjectMarkdown } from './ProjectMarkdown';

function ProjectLinks({ project }: { project: ProjectDetail }) {
  const { t } = useTranslation();
  const links = (
    [
      ['source', project.sourceUrl],
      ['issues', project.issuesUrl],
      ['wiki', project.wikiUrl],
    ] as const
  ).filter((entry): entry is readonly ['source' | 'issues' | 'wiki', string] => entry[1] !== null);
  if (links.length === 0 && !project.license) return null;
  return (
    <section className="grid gap-2" aria-label={t('project.links')}>
      <h2 className="text-sm font-semibold">{t('project.links')}</h2>
      <ul className="grid justify-items-start gap-1">
        {links.map(([key, url]) => (
          <li key={key}>
            <Button variant="ghost" icon={<ExternalLink />} onClick={() => void commands.openLink(url).catch(pushError)}>
              {t(`project.${key}`)}
            </Button>
          </li>
        ))}
      </ul>
      {project.license && <p className="text-sm text-ink-dim">{t('project.license', { license: project.license })}</p>}
    </section>
  );
}

export function ProjectPage({ route, project }: { route: BrowseRoute; project: string }) {
  const { t, i18n } = useTranslation();
  const detail = useBrowseProject(project);
  const back = () => navigate({ name: 'browse', kind: route.kind, instance: route.instance });
  const frame = (title: string, children: ReactNode, subtitle?: string) => (
    <ScreenFrame title={title} subtitle={subtitle} wide onBack={back} backLabel={t('project.back')}>
      {children}
    </ScreenFrame>
  );
  if (detail.isPending) return frame(t('browse.title'), <p role="status">{t('common.loading')}</p>);
  if (detail.isError)
    return frame(
      t('browse.title'),
      <ErrorCard error={detail.error} actions={<Button onClick={() => void detail.refetch()}>{t('common.retry')}</Button>} />,
    );
  const p = detail.data;
  const lang = i18n.language;
  const installSlot = <ActionPlate project={p} route={route} />;
  return frame(
    p.title,
    <div className="project-layout">
      <div className="grid min-w-0 content-start gap-6">
        <div className="flex items-start gap-4">
          <ProjectIcon seed={p.projectId} url={p.iconUrl} size={64} />
          <div className="grid gap-2">
            <p>{p.summary}</p>
            <p className="tabular text-sm text-ink-dim">
              {t('browse.downloads', { count: p.downloads, shown: formatCompact(p.downloads, lang) })}
              {' · '}
              {t('browse.follows', { count: p.follows, shown: formatCompact(p.follows, lang) })}
            </p>
            {p.categories.length > 0 && (
              <p className="flex flex-wrap gap-1.5">
                {p.categories.map((c) => (
                  <span key={c} className="tag">
                    {categoryLabel(i18n, c)}
                  </span>
                ))}
              </p>
            )}
            {p.serverSide === 'required' && <p className="text-sm text-blue-ink">{t('project.serverToo')}</p>}
          </div>
        </div>
        <Gallery images={p.gallery} />
        <ProjectMarkdown source={p.body} />
      </div>
      <aside className="grid content-start gap-6">
        {installSlot}
        <ProjectLinks project={p} />
      </aside>
    </div>,
    p.author ? t('browse.by', { author: p.author }) : undefined,
  );
}
