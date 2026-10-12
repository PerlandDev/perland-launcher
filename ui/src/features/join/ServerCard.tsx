import { useTranslation } from 'react-i18next';
import { PlotIcon } from '../../art/PlotIcon';
import type { JoinPreview } from '../../ipc/schema';
import { Motd } from '../servers/MotdText';
import { StatusPill } from '../servers/StatusPill';

/** What the invite says about the server. Nothing here is lit: the screen's button is. */
export function ServerCard({ preview }: { preview: JoinPreview }) {
  const { t } = useTranslation();
  const version = preview.game.version;
  const pack = preview.pack;
  return (
    <article className="panel grid gap-4 p-6" aria-labelledby="server-name">
      <div className="flex items-start gap-5">
        {preview.icon ? (
          <img src={preview.icon} alt="" width={64} height={64} className="server-icon" draggable={false} />
        ) : (
          <PlotIcon seed={preview.name} size={64} />
        )}
        <div className="grid min-w-0 gap-2">
          <div className="flex flex-wrap items-center gap-3">
            <h2 id="server-name" className="font-display text-2xl [overflow-wrap:anywhere]">
              {preview.name}
            </h2>
            {preview.team?.accent && (
              <span className="team-mark" data-accent={preview.team.accent} title={preview.team.displayName || undefined} aria-hidden />
            )}
          </div>
          <Motd text={preview.motd} />
          <StatusPill status={preview.status} players={preview.players} gone={false} />
        </div>
      </div>
      <dl className="grid gap-1">
        {version && (
          <div>
            <dt className="sr-only">{t('join.versionLabel')}</dt>
            <dd>{pack ? t('join.versionLine', { version }) : t('join.vanilla', { version })}</dd>
          </div>
        )}
        {pack && (
          <div>
            <dt className="sr-only">{t('join.packLabel')}</dt>
            <dd>{`${[pack.name, pack.version].filter(Boolean).join(' ')} · ${t('join.packMods', { count: pack.modCount })}`}</dd>
          </div>
        )}
      </dl>
      <p className="text-sm text-ink-dim">{t('join.from', { host: preview.host })}</p>
    </article>
  );
}
