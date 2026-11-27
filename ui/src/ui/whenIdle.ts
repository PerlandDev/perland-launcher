/** How long a busy window may put the work off once the delay is over. */
const IDLE_TIMEOUT_MS = 2000;
/** Where the webview has no idle callback (WebKitGTK), the work waits this much longer instead. */
const NO_IDLE_WAIT_MS = 1000;

/** Runs `work` once, `delayMs` from now, when the window next has nothing better to do. Returns the cancel. */
export function whenIdle(work: () => void, delayMs: number): () => void {
  let idle: number | null = null;
  let later: ReturnType<typeof setTimeout> | null = null;
  const start = setTimeout(() => {
    if (typeof requestIdleCallback === 'function') idle = requestIdleCallback(work, { timeout: IDLE_TIMEOUT_MS });
    else later = setTimeout(work, NO_IDLE_WAIT_MS);
  }, delayMs);
  return () => {
    clearTimeout(start);
    if (later !== null) clearTimeout(later);
    if (idle !== null && typeof cancelIdleCallback === 'function') cancelIdleCallback(idle);
  };
}
