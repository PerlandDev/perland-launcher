import { useTranslation } from 'react-i18next';
import { useAccountReady, useDismissOfflineNotice } from '../../state/queries';
import { Button } from '../../ui/Button';
import { MicrosoftSignIn } from '../accounts/MicrosoftSignIn';

/** For a player of an older beta whose offline accounts this build hides: a note plate (the blue info lamp: it's news, not a
 *  fault) saying so, with the way on (the Microsoft sign-in, the one lit key while there's no account to play with) and a
 *  ghost Dismiss that tells the core to stop asking. Nothing renders unless the core asks for it. */
export function OfflineAccountsNotice({ className = '' }: { className?: string }) {
  const { t } = useTranslation();
  const ready = useAccountReady();
  const dismiss = useDismissOfflineNotice();
  if (!ready.data?.offlineNotice || dismiss.isSuccess) return null;
  return (
    <div className={`note-plate max-w-[36rem] ${className}`}>
      <div className="grid min-w-0 flex-1 gap-4">
        <p>{t('home.offlineGone')}</p>
        <div className="flex flex-wrap items-start gap-3">
          <div className="w-full max-w-[18rem]">
            <MicrosoftSignIn variant="primary" showWhy={false} />
          </div>
          <Button variant="ghost" size="lg" disabled={dismiss.isPending} onClick={() => dismiss.mutate()}>
            {t('common.dismiss')}
          </Button>
        </div>
      </div>
    </div>
  );
}
