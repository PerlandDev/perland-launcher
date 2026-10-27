import { mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it } from 'vitest';
import { requireAccount } from './requireAccount';
import { closeSignInPrompt, signInPromptSnapshot } from './signInPrompt';

afterEach(closeSignInPrompt);

const answer = (ready: boolean) => () =>
  mockIPC((cmd) => (cmd === 'account_ready' ? { ready } : null));

describe('requireAccount', () => {
  it('opens the prompt and answers false when no account can play', async () => {
    answer(false)();
    expect(await requireAccount()).toBe(false);
    expect(signInPromptSnapshot()).toBe(true);
  });

  it('answers true when an account can play, leaving the prompt closed', async () => {
    answer(true)();
    expect(await requireAccount()).toBe(true);
    expect(signInPromptSnapshot()).toBe(false);
  });

  it('answers true when the check fails: the launch check still decides', async () => {
    mockIPC((cmd) => {
      if (cmd === 'account_ready') throw new Error('no answer');
      return null;
    });
    expect(await requireAccount()).toBe(true);
    expect(signInPromptSnapshot()).toBe(false);
  });

  it('asks every time, never from a cache', async () => {
    let ready = false;
    let asked = 0;
    mockIPC((cmd) => {
      if (cmd !== 'account_ready') return null;
      asked++;
      return { ready };
    });
    expect(await requireAccount()).toBe(false);
    ready = true;
    expect(await requireAccount()).toBe(true);
    expect(asked).toBe(2);
  });
});
