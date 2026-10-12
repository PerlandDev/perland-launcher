import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import type { SearchHit } from '../../ipc/schema';
import { formatCompact } from '../../ui/format';
import { categoryLabel } from './labels';
import { ProjectIcon } from './ProjectIcon';

export function ResultRow({
  hit,
  scoped,
  installed,
  onOpen,
  action,
}: {
  hit: SearchHit;
  /** Browse is adding to one instance: show its lamp or an Install button. */
  scoped: boolean;
  installed: boolean;
  onOpen: () => void;
  action?: ReactNode;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  return (
    <li className="browse-row">
      <button type="button" className="browse-row-open" onClick={onOpen}>
        <ProjectIcon seed={hit.projectId} url={hit.iconUrl} />
        <span className="grid min-w-0 gap-1 text-left">
          <span className="font-display text-lg leading-tight">{hit.title}</span>
          {hit.author && <span className="text-sm text-ink-dim">{t('browse.by', { author: hit.author })}</span>}
          <span className="line-clamp-2 text-ink-dim">{hit.summary}</span>
          <span className="tabular text-sm text-ink-dim">
            {t('browse.downloads', { count: hit.downloads, shown: formatCompact(hit.downloads, lang) })}
            {' · '}
            {t('browse.follows', { count: hit.follows, shown: formatCompact(hit.follows, lang) })}
          </span>
          {hit.categories.length > 0 && (
            <span className="flex flex-wrap gap-1.5">
              {hit.categories.map((c) => (
                <span key={c} className="tag">
                  {categoryLabel(i18n, c)}
                </span>
              ))}
            </span>
          )}
        </span>
      </button>
      {scoped && (installed ? <span className="installed-lamp">{t('browse.installed')}</span> : action)}
    </li>
  );
}
