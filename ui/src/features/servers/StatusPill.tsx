import { useTranslation } from 'react-i18next';
import type { ServerPlayers, ServerStatus } from '../../ipc/schema';
import { statusWord } from './statusWord';

/** A square LED and a word: the status never relies on colour alone. A list passes its own `word` and
 *  `live={false}`, so a page of cards isn't a page of live regions announcing every refresh. */
export function StatusPill({
  status,
  players,
  gone,
  word,
  live = true,
}: {
  status: ServerStatus | null;
  players: ServerPlayers | null;
  gone: boolean;
  word?: string;
  live?: boolean;
}) {
  const { t } = useTranslation();
  const text = word ?? statusWord(t, status, players, gone);
  return (
    <span
      className="status-pill"
      data-status={gone || !status ? 'unknown' : status}
      role={live ? 'status' : undefined}
      aria-label={live ? t('server.statusLabel', { status: text }) : undefined}
    >
      <span className="status-led" aria-hidden />
      {text}
    </span>
  );
}
