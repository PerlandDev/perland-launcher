import type { GameWindow } from '../../ipc/schema';

/** The core's limits (`MIN_WINDOW`, `MAX_WINDOW_SIDE`), so Save waits instead of being refused. */
export const MIN_WINDOW = { width: 320, height: 240 } as const;
export const MAX_WINDOW_SIDE = 16384;
/** What "Custom size" starts from when the instance had none. */
const STARTING_SIZE = { width: 1280, height: 720 } as const;

export type WindowMode = GameWindow['mode'];
/** The fields as typed: text, so a cleared field stays empty instead of turning into 0. */
export type WindowDraft = { mode: WindowMode; width: string; height: string };

export function windowDraft(window: GameWindow): WindowDraft {
  const size = window.mode === 'size' ? window : STARTING_SIZE;
  return { mode: window.mode, width: String(size.width), height: String(size.height) };
}

/** A typed side as a number, or null when it isn't a whole number from `min` to `MAX_WINDOW_SIDE`. */
export function side(text: string, min: number): number | null {
  if (!/^\d{1,5}$/.test(text)) return null;
  const n = Number(text);
  return n >= min && n <= MAX_WINDOW_SIDE ? n : null;
}

/** The window to save, or null while a typed size is out of range. */
export function windowOf(draft: WindowDraft): GameWindow | null {
  if (draft.mode !== 'size') return { mode: draft.mode };
  const width = side(draft.width, MIN_WINDOW.width);
  const height = side(draft.height, MIN_WINDOW.height);
  return width !== null && height !== null ? { mode: 'size', width, height } : null;
}
