import { commands } from '../ipc/commands';
import type { RunSnapshot } from '../ipc/schema';
import { runStore, type RunStore } from './runStore';
import { pushError } from './toasts';

/**
 * After a reload, pick up games that are still running and their recent output.
 * Never throws: whatever could be read is used, and the first failure is shown once.
 */
export async function hydrateRuns(store: RunStore = runStore): Promise<void> {
  let snapshots: RunSnapshot[];
  try {
    snapshots = await commands.runStates();
  } catch (error) {
    pushError(error);
    return;
  }
  // States first, so the window for a game to change under us is only the log reads.
  store.hydrate(snapshots, {});
  const results = await Promise.allSettled(
    snapshots.map(async (s) => [s.instance, await commands.logTail(s.instance)] as const),
  );
  const tails: Record<string, string[]> = {};
  const failures: unknown[] = [];
  for (const result of results) {
    if (result.status === 'fulfilled') tails[result.value[0]] = result.value[1];
    else failures.push(result.reason);
  }
  store.hydrate([], tails);
  if (failures.length > 0) pushError(failures[0]);
}
