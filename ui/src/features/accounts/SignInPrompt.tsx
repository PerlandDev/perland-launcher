import { useTranslation } from 'react-i18next';
import { closeSignInPrompt, useSignInPrompt } from '../../state/signInPrompt';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { MicrosoftSignIn } from './MicrosoftSignIn';
import { pushSignedInNotice } from './signedInNotice';

/** The one place Play asks for a Microsoft account: the standard dialog, opened from anywhere (a refused launch, the check
 *  before a join or a free server's start) and drawn once by the app. The sign-in is its one lit key; Close is ghost. Once
 *  signed in it closes and a notice says to press Play again, since the Play that opened it didn't happen. */
export function SignInPrompt() {
  const { t } = useTranslation();
  const open = useSignInPrompt();
  const signedIn = () => {
    closeSignInPrompt();
    pushSignedInNotice(t);
  };
  return (
    <Dialog
      open={open}
      onClose={closeSignInPrompt}
      title={t('accounts.promptTitle')}
      actions={
        <Button variant="ghost" onClick={closeSignInPrompt}>
          {t('common.close')}
        </Button>
      }
    >
      <p>{t('accounts.promptBody')}</p>
      <div className="mt-5 text-ink">
        <MicrosoftSignIn variant="primary" showWhy={false} onSignedIn={signedIn} />
      </div>
    </Dialog>
  );
}
