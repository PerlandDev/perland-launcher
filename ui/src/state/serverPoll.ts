import { commands } from '../ipc/commands';
import { isLauncherError } from '../ipc/errors';
import type { ServerStatusView } from '../ipc/schema';

/** Status polls: the selected instance while the window is focused, and the rail. */
export const SELECTED_POLL = 30_000;
export const RAIL_POLL = 60_000;
const MAX_POLL = 120_000;

/** Rate limits in a row, per instance. A success clears them. */
const strikes = new Map<string, number>();

/** The next poll: `base`, doubled for each rate limit in a row, at most 2 minutes. */
export function nextPoll(base: number, limited: number): number {
  return limited > 0 ? Math.min(base * 2 ** limited, MAX_POLL) : base;
}

export function strikesOf(id: string): number {
  return strikes.get(id) ?? 0;
}

export async function pollServerStatus(id: string): Promise<ServerStatusView> {
  try {
    const status = await commands.serverStatus(id);
    strikes.delete(id);
    return status;
  } catch (error) {
    if (isLauncherError(error) && error.code === 'invite_rate_limited') strikes.set(id, strikesOf(id) + 1);
    throw error;
  }
}

export function resetServerPolls() {
  strikes.clear();
}
