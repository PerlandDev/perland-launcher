import { useSyncExternalStore } from 'react';
import type { AppEvent, ExitInfo, PackUpdate, RunSnapshot, RunStateKind, TaskId } from '../ipc/schema';

/** Lines kept per instance, matching the Rust side's log_tail. */
export const LOG_CAP = 5000;

export interface ProgressView {
  task: TaskId;
  done: number;
  total: number;
  bytes: number;
}

export interface RunView {
  state: RunStateKind;
  progress: ProgressView | null;
  lastExit: ExitInfo | null;
  /** The pack update on offer; set only while the run is asking. */
  update: PackUpdate | null;
}

const IDLE: RunView = { state: 'idle', progress: null, lastExit: null, update: null };
const NO_LINES: string[] = [];

export function createRunStore() {
  let runs = new Map<string, RunView>();
  let logs = new Map<string, string[]>();
  let activeIds: string[] = [];
  // Instances whose run state arrived as a live event; a later reload snapshot is older than that.
  const heardLive = new Set<string>();
  const listeners = new Set<() => void>();

  const notify = () => listeners.forEach((listener) => listener());

  function setRun(id: string, update: (run: RunView) => RunView) {
    const next = new Map(runs);
    next.set(id, update(runs.get(id) ?? IDLE));
    runs = next;
    activeIds = [...runs].filter(([, run]) => run.state !== 'idle').map(([key]) => key);
  }

  function setLog(id: string, lines: string[]) {
    const next = new Map(logs);
    next.set(id, lines.length > LOG_CAP ? lines.slice(lines.length - LOG_CAP) : lines);
    logs = next;
  }

  return {
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    run: (id: string): RunView => runs.get(id) ?? IDLE,
    log: (id: string): string[] => logs.get(id) ?? NO_LINES,
    active: (): string[] => activeIds,
    apply(event: AppEvent) {
      switch (event.type) {
        case 'progress': {
          const id = event.instance;
          if (!id) return;
          const { task, done, total, bytes } = event;
          setRun(id, (run) => ({ ...run, progress: { task, done, total, bytes } }));
          break;
        }
        case 'log':
          setLog(event.instance, (logs.get(event.instance) ?? NO_LINES).concat(event.lines));
          break;
        case 'run_state': {
          const { instance: id, state, exit, update } = event;
          heardLive.add(id);
          // A run starts when an idle instance reports anything else: checking or queued.
          const starting = (runs.get(id)?.state ?? 'idle') === 'idle' && state !== 'idle';
          if (starting) setLog(id, NO_LINES);
          setRun(id, (run) => ({
            state,
            progress: state === run.state ? run.progress : null,
            lastExit: starting ? null : (exit ?? run.lastExit),
            update: state === 'asking' ? (update ?? run.update) : null,
          }));
          break;
        }
        case 'warning':
          return; // shown as toasts, see handleEvent
      }
      notify();
    },
    hydrate(snapshots: RunSnapshot[], tails: Record<string, string[]>) {
      // Snapshots and tails are read after live events started arriving, so anything already heard live is newer.
      for (const snapshot of snapshots) {
        if (!heardLive.has(snapshot.instance)) setRun(snapshot.instance, (run) => ({ ...run, state: snapshot.state, update: snapshot.update }));
      }
      for (const [id, lines] of Object.entries(tails)) {
        if (!logs.has(id)) setLog(id, lines);
      }
      notify();
    },
    reset() {
      runs = new Map();
      logs = new Map();
      activeIds = [];
      heardLive.clear();
      notify();
    },
    dismissExit(id: string) {
      setRun(id, (run) => ({ ...run, lastExit: null }));
      notify();
    },
  };
}

export type RunStore = ReturnType<typeof createRunStore>;

export const runStore = createRunStore();

export function useRun(id: string): RunView {
  return useSyncExternalStore(runStore.subscribe, () => runStore.run(id));
}

export function useLog(id: string): string[] {
  return useSyncExternalStore(runStore.subscribe, () => runStore.log(id));
}

export function useActiveIds(): string[] {
  return useSyncExternalStore(runStore.subscribe, runStore.active);
}
