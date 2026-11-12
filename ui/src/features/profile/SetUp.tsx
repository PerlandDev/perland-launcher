import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { useAccounts, usePerlandStatus } from '../../state/queries';
import { Check } from '../../ui/Icon';
import { MicrosoftSignIn } from '../accounts/MicrosoftSignIn';
import { PerlandStep } from './PerlandStep';

/** What is still to connect, read from what the launcher already knows: a Microsoft account while none is added, and the
 *  Perland account while signed out (never while the Engine has player accounts closed). */
function useSetUp() {
  const accounts = useAccounts();
  const perland = usePerlandStatus();
  const microsoft = accounts.isSuccess && !accounts.data.accounts.some((a) => a.provider === 'microsoft');
  const perlandOpen = perland.data?.enabled === true && perland.data.account === null;
  return { microsoft, perland: perlandOpen, open: microsoft || perlandOpen };
}

/** Finish setting up: the Profile's checklist of what is still to connect, heading the page so a signed-out player can't
 *  miss it. Who is playing is its one done line; then a step each for the Microsoft account (the page's one lit key) and the
 *  Perland account, each with what it is for and its key. A step goes once it is done, and the whole panel goes with the
 *  last one. `onMicrosoft` and `onPerland` hear a sign-in, since its step is gone by then. */
export function SetUp({ onMicrosoft, onPerland }: { onMicrosoft: () => void; onPerland: (name: string) => void }) {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const steps = useSetUp();
  const titleId = useId();
  const microsoftId = useId();
  if (!steps.open) return null;
  const active = accounts.data?.accounts.find((a) => a.uuid === accounts.data?.active) ?? null;
  return (
    <section className="panel setup" aria-labelledby={titleId}>
      <h2 id={titleId} className="profile-title">
        {t('profile.setup.title')}
      </h2>
      <ul className="setup-steps">
        {active && (
          <li className="setup-step">
            <Check className="setup-done" aria-hidden />
            <div className="setup-what">
              <p className="font-bold">
                {t('profile.setup.playingAs', { name: active.username })}
                <span className="sr-only">: {t('profile.setup.done')}</span>
              </p>
              <p className="text-sm text-ink-dim">{active.provider === 'microsoft' ? t('accounts.kindMicrosoft') : t('accounts.kindOffline')}</p>
            </div>
          </li>
        )}
        {steps.microsoft && (
          <li className="setup-step" data-step="microsoft" aria-labelledby={microsoftId}>
            <span className="setup-todo" aria-hidden />
            <div className="setup-what">
              <h3 id={microsoftId} className="font-bold">
                {t('accounts.kindMicrosoft')}
                <span className="sr-only">: {t('profile.setup.todo')}</span>
              </h3>
              <p className="text-sm text-ink-dim">{t('profile.setup.microsoftWhy')}</p>
            </div>
            <div className="setup-act">
              <MicrosoftSignIn variant="step" showWhy={false} onSignedIn={onMicrosoft} />
            </div>
          </li>
        )}
        {steps.perland && <PerlandStep onSignedIn={onPerland} />}
      </ul>
    </section>
  );
}
