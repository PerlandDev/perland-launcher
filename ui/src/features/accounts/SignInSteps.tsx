import { useTranslation } from 'react-i18next';
import { MSA_STEPS, type MsaStep } from '../../ipc/schema';
import { Check, Clock, WarningDiamond } from '../../ui/Icon';
import { stepStates, type StepState, type StepsEnd } from './stepStates';

/** The calls of the Microsoft sign-in chain as a list of lamps: what passed, what runs now, and where a refusal stopped it. Every
 *  state is said in words too (screen readers read "Xbox Live: done"), never by hue alone. */
export function SignInSteps({ reached, end }: { reached: MsaStep; end: StepsEnd }) {
  const { t } = useTranslation();
  const states = stepStates(reached, end);
  return (
    <ol aria-label={t('accounts.steps.label')} className="signin-steps">
      {MSA_STEPS.map((step, i) => {
        const state = states[i]!;
        return (
          <li key={step} data-state={state}>
            <StepMark state={state} />
            <span>
              {t(`accounts.steps.${step}`)}
              <span className="sr-only">: {t(`accounts.steps.state.${state}`)}</span>
            </span>
          </li>
        );
      })}
    </ol>
  );
}

function StepMark({ state }: { state: StepState }) {
  switch (state) {
    case 'done':
      return <Check aria-hidden className="signin-step-icon text-lime" />;
    case 'fault':
      return <WarningDiamond aria-hidden className="signin-step-icon text-coral" />;
    case 'waiting':
      return <Clock aria-hidden className="signin-step-icon text-blue-ink" />;
    default:
      return <span aria-hidden className="signin-step-lamp" />;
  }
}
