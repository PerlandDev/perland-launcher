import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { PlotIcon } from '../../art/PlotIcon';
import type { JoinPreview } from '../../ipc/schema';
import { Motd } from '../servers/MotdText';
import { StatusPill } from '../servers/StatusPill';
import { versionWord } from '../servers-list/labels';
import { useServerIconSize } from './iconScale';

/** What the invite says about the server. Nothing here is lit: the screen's button is. */
export function ServerCard({ preview }: { preview: JoinPreview }) {
  const { t } = useTranslation();
  // An icon that fails to decode gives the card the generated plot icon, as on the rail.
  const [failed, setFailed] = useState<string | null>(null);
  const icon = preview.icon && preview.icon !== failed ? preview.icon : null;
  const size = useServerIconSize();
  const version = versionWord(t, preview.game.version);
  const pack = preview.pack;
  return (
    <article className="panel grid gap-5 p-6" aria-labelledby="server-name">
      <div className="flex items-center gap-6">
        {/* A server icon is 64 × 64: sized to a whole number of device pixels per icon pixel. */}
        <span className="server-icon">
          {icon ? (
            <img src={icon} alt="" style={{ width: size, height: size }} draggable={false} onError={() => setFailed(icon)} />
          ) : (
            <PlotIcon seed={preview.name} size={size} />
          )}
        </span>
        <div className="grid min-w-0 justify-items-start gap-2.5">
          <div className="flex min-w-0 flex-wrap items-center gap-3">
            <h2 id="server-name" className="font-display text-[1.75rem] leading-none [overflow-wrap:anywhere]">
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
      <div className="grid gap-1 border-t border-line pt-4">
        <dl className="contents">
          {version && (
            <div>
              <dt className="sr-only">{t('join.versionLabel')}</dt>
              <dd className="font-semibold">{pack ? t('join.versionLine', { version }) : t('join.vanilla', { version })}</dd>
            </div>
          )}
          {pack && (
            <div>
              <dt className="sr-only">{t('join.packLabel')}</dt>
              <dd className="tabular">{`${[pack.name, pack.version].filter(Boolean).join(' ')} · ${t('join.packMods', { count: pack.modCount })}`}</dd>
            </div>
          )}
        </dl>
        <p className="text-sm text-ink-dim">{t('join.from', { host: preview.host })}</p>
      </div>
    </article>
  );
}
