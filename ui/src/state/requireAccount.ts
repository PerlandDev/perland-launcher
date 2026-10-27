import { commands } from '../ipc/commands';
import { openSignInPrompt } from './signInPrompt';

/** Before anything that leads to playing (a join, a free server's start): whether an account can play. When none can, the
 *  sign-in prompt opens and the answer is false, so nothing is added or started for a game that couldn't launch. Asked fresh
 *  every time, straight from the core (no network call there), never from a cache. A check that fails answers true: the
 *  launch's own check still decides, and its refusal opens the same prompt. */
export async function requireAccount(): Promise<boolean> {
  let ready: boolean;
  try {
    ready = (await commands.accountReady()).ready;
  } catch {
    return true;
  }
  if (!ready) openSignInPrompt();
  return ready;
}
