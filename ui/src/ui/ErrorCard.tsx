import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { describeError } from '../ipc/errors';
import { Button } from './Button';
import { WarningDiamond } from './Icon';

/** A failure, in the player's language: what happened, what to do, details on request. */
export function ErrorCard({ error, actions }: { error: unknown; actions?: ReactNode }) {
  const { t, i18n } = useTranslation();
  const [showDetails, setShowDetails] = useState(false);
  const text = describeError(i18n, error);
  return (
    <div role="alert" className="panel grid gap-3 p-5">
      <div className="flex items-start gap-3">
        <WarningDiamond className="mt-0.5 h-6 w-6 flex-none text-coral" aria-hidden />
        <div className="grid gap-1">
          <p className="font-semibold">{text.message}</p>
          {text.fix && <p className="text-ink-dim">{text.fix}</p>}
        </div>
      </div>
      <div className="flex flex-wrap gap-3">
        {actions}
        {text.detail && (
          <Button variant="ghost" aria-expanded={showDetails} onClick={() => setShowDetails((v) => !v)}>
            {showDetails ? t('common.hideDetails') : t('common.details')}
          </Button>
        )}
      </div>
      {showDetails && text.detail && (
        <pre className="selectable max-h-48 overflow-auto rounded bg-ground p-3 font-mono text-sm whitespace-pre-wrap text-ink-dim">
          {text.detail}
        </pre>
      )}
    </div>
  );
}
