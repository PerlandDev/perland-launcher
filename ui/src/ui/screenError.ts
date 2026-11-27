import type { LauncherError } from '../ipc/schema';

/** The launcher's own failures that never come from the core: a screen that broke or didn't load. */
export type ScreenErrorCode = 'skins_open_failed' | 'screen_failed';

/** A failure caught in the UI, in the shape the error card translates by `code`; the cause is its detail. */
export function screenError(code: ScreenErrorCode, cause: unknown): LauncherError {
  return {
    // The category is the core's grouping and is never shown; a file that didn't load or draw is the closest.
    category: 'integrity',
    code,
    message: code === 'skins_open_failed' ? "The skin studio didn't open." : 'This screen ran into a problem.',
    fix: null,
    detail: cause instanceof Error ? cause.message : String(cause),
    params: {},
  };
}
