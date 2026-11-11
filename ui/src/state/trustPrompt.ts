import { useSyncExternalStore } from 'react';
import type { TrustPrompt } from '../ipc/schema';

/** The one trust step dialog, drawn once by the app. Two ways in:
 *  - `instance`: a Play was refused with `server_trust_needed`; the dialog reads the step for that instance, records the
 *    answer and plays again (joining when the refused Play would have).
 *  - `join`: a server list's Play, before anything is added; the caller gets the answer (whether the pack updates on its
 *    own), or null for Not now. */
export type TrustRequest =
  | { kind: 'instance'; id: string; join: boolean }
  | { kind: 'join'; prompt: TrustPrompt; resolve: (autoUpdate: boolean | null) => void };

let request: TrustRequest | null = null;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((listener) => listener());

function open(next: TrustRequest) {
  // A second ask while one is open answers the first with Not now: only one step is on screen.
  if (request?.kind === 'join') request.resolve(null);
  request = next;
  notify();
}

/** Show the step for instance `id` (a refused Play). */
export function askInstanceTrust(id: string, join: boolean) {
  if (request?.kind === 'instance' && request.id === id) return;
  open({ kind: 'instance', id, join });
}

/** Show `prompt` and wait for the answer: whether the pack updates on its own, or null for Not now. */
export function askJoinTrust(prompt: TrustPrompt): Promise<boolean | null> {
  return new Promise((resolve) => open({ kind: 'join', prompt, resolve }));
}

/** Close the step; a waiting join hears `answer`. */
export function closeTrust(answer: boolean | null = null) {
  if (!request) return;
  if (request.kind === 'join') request.resolve(answer);
  request = null;
  notify();
}

export function trustSnapshot(): TrustRequest | null {
  return request;
}

export function useTrustRequest(): TrustRequest | null {
  return useSyncExternalStore((listener) => {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, trustSnapshot);
}
