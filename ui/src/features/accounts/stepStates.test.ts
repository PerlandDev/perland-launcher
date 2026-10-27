import { describe, expect, it } from 'vitest';
import { stepStates } from './stepStates';

describe('stepStates', () => {
  it('marks the steps before the one reached done and the ones after it not reached', () => {
    expect(stepStates('xsts', 'running')).toEqual(['done', 'done', 'running', 'pending', 'pending']);
  });

  it('ends on the step reached as the refusal says', () => {
    expect(stepStates('minecraft', 'waiting')).toEqual(['done', 'done', 'done', 'waiting', 'pending']);
    expect(stepStates('microsoft', 'fault')).toEqual(['fault', 'pending', 'pending', 'pending', 'pending']);
    expect(stepStates('profile', 'fault')).toEqual(['done', 'done', 'done', 'done', 'fault']);
  });
});
