import { commands } from '../ipc/commands';

/** Reports per run of the launcher: an error in a render loop must not become a stream of calls. */
export const REPORT_CAP = 10;
const MESSAGE_MAX = 500;
const STACK_MAX = 8192;

type Kind = 'exception' | 'unhandled_rejection';
type Send = (kind: Kind, message: string, stack: string | null) => Promise<unknown>;

/** The message and stack of whatever was thrown. Only an Error's own text: never an object's fields. */
function describe(thrown: unknown): { message: string; stack: string | null } {
  if (thrown instanceof Error) {
    return { message: `${thrown.name}: ${thrown.message}`, stack: thrown.stack ?? null };
  }
  if (typeof thrown === 'string') return { message: thrown, stack: null };
  // A LauncherError a command answered with was already reported where the command failed.
  return { message: `Non-error thrown (${typeof thrown})`, stack: null };
}

/** Is this an error a backend command answered with (reported by Rust already)? */
function isLauncherError(thrown: unknown): boolean {
  return typeof thrown === 'object' && thrown !== null && 'code' in thrown && 'category' in thrown;
}

/**
 * Hands the window's own uncaught errors to Rust for the anonymous error reports. Rust decides whether anything is
 * kept (the Settings switch) and scrubs it; here they are only deduplicated and capped. Reporting never throws and
 * never shows anything. Returns the function that stops it.
 */
export function installErrorReporting(target: Window = window, send: Send = commands.telemetryReport): () => void {
  const seen = new Set<string>();
  let sent = 0;
  const report = (kind: Kind, thrown: unknown) => {
    try {
      if (isLauncherError(thrown)) return;
      const { message, stack } = describe(thrown);
      const key = `${kind}\n${message}\n${stack ?? ''}`;
      if (seen.has(key) || sent >= REPORT_CAP) return;
      seen.add(key);
      sent += 1;
      void send(kind, message.slice(0, MESSAGE_MAX), stack?.slice(0, STACK_MAX) ?? null).catch(() => {});
    } catch {
      // Reporting an error must never be one.
    }
  };
  const onError = (event: ErrorEvent) => report('exception', event.error ?? event.message);
  const onRejection = (event: PromiseRejectionEvent) => report('unhandled_rejection', event.reason);
  target.addEventListener('error', onError);
  target.addEventListener('unhandledrejection', onRejection);
  return () => {
    target.removeEventListener('error', onError);
    target.removeEventListener('unhandledrejection', onRejection);
  };
}
