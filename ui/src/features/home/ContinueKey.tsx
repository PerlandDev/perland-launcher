import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { useContinueTarget } from '../../state/queries';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Map as WorldIcon, Server } from '../../ui/Icon';

/** Straight back into the last world or server, from Play's column. A raised key, never lit: Play stays the one
 *  amber button on Home. Nowhere to continue (never played, the world gone, the server off the list) means no key. */
export function ContinueKey({
  id,
  disabled,
  once,
}: {
  id: string;
  disabled: boolean;
  /** Play's single-flight, shared so a Continue and a Play can't both start a run. */
  once: (start: (done: () => void) => void) => boolean;
}) {
  const { t } = useTranslation();
  const target = useContinueTarget(id).data;
  if (!target) return null;
  const label = t('home.continueTo', { name: target.name });
  return (
    <Button
      className="max-w-[17.5rem]"
      icon={target.kind === 'world' ? <WorldIcon /> : <Server />}
      disabled={disabled}
      aria-label={label}
      title={label}
      onClick={() => once((done) => void commands.playContinue(id).catch(pushError).finally(done))}
    >
      {/* The verb stays whole and the name gives way: a long world name ends in an ellipsis, never the verb. */}
      <span className="flex-none font-normal text-ink-dim">{t('home.continue')}</span>
      <span className="min-w-0 truncate">{target.name}</span>
    </Button>
  );
}
