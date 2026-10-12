import { useTranslation } from 'react-i18next';
import type { ServerPlayers, ServerStatus } from '../../ipc/schema';
import { statusWord } from './statusWord';

/** A square LED and a word: the status never relies on colour alone. */
export function StatusPill({ status, players, gone }: { status: ServerStatus | null; players: ServerPlayers | null; gone: boolean }) {
  const { t } = useTranslation();
  const word = statusWord(t, status, players, gone);
  return (
    <span className="status-pill" data-status={gone || !status ? 'unknown' : status} role="status" aria-label={t('server.statusLabel', { status: word })}>
      <span className="status-led" aria-hidden />
      {word}
    </span>
  );
}
