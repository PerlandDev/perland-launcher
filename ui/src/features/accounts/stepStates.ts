import { MSA_STEPS, type MsaStep } from '../../ipc/schema';

/** How a sign-in ended on the step it reached: still running, refused by a fault, or turned down while Minecraft hasn't approved
 *  the app yet (info, not a fault). */
export type StepsEnd = 'running' | 'fault' | 'waiting';

export type StepState = 'done' | 'running' | 'pending' | 'fault' | 'waiting';

/** Each step's state: the ones before the step reached are done, the reached one is running or ended as `end` says, the rest were
 *  not reached. */
export function stepStates(reached: MsaStep, end: StepsEnd): StepState[] {
  const at = MSA_STEPS.indexOf(reached);
  return MSA_STEPS.map((_, i) => (i < at ? 'done' : i > at ? 'pending' : end));
}
