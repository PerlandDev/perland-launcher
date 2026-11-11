// Try before you wear: pointing at a piece, a look or an expression on the Create tab shows it on the figure without
// changing the skin. `tryState` is the look a change would give, worked out off the document; `TryOn` decides when a
// pointed tile is loaded and shown (after a short wait, so a pointer sweeping the grid loads nothing; the newest tile
// wins; leaving goes back to the real skin).
import type { FocusEvent, PointerEvent } from 'react';
import type { SkinLook, SkinModel } from '../../ipc/schema';
import type { LookState } from './history';
import type { PixelBuffer } from './recolour';

/** How long a tile stays pointed before its preview starts, and how long the preview lingers once it is left. */
export const TRY_DELAY_MS = 120;

/** Something that can be tried on: `key` tells tiles apart, `name` is what the stage's Preview badge reads. */
export interface Candidate {
  key: string;
  name: string;
  change: (look: SkinLook, model: SkinModel) => SkinLook;
}

/**
 * The look and painting the open skin would have with `change` made, for `compose`; null when the change hands the
 * same look back. A plain skin's own texels are the painting its first piece goes around (as the editor starts a look).
 * Nothing given is changed.
 */
export function tryState(now: { look: LookState | null; pixels: PixelBuffer }, change: (look: SkinLook) => SkinLook): { look: SkinLook; painting: PixelBuffer | null } | null {
  const from: SkinLook = now.look?.look ?? { pieces: [], painting: true };
  const look = change(from);
  if (look === from) return null;
  return { look, painting: now.look ? now.look.painting : now.pixels };
}

export function sameTexels(a: PixelBuffer, b: PixelBuffer): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/** Whether a pointer of this type previews by hovering: a finger has no hover, and its tap wears the piece. */
export function hoverPreviews(pointerType: string): boolean {
  if (pointerType === 'touch') return false;
  return !(typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia('(hover: none)').matches);
}

/** What the Create tab's tiles call. */
export interface TryOnHandle {
  /** The pointer or the keyboard is on this tile. */
  point(candidate: Candidate): void;
  /** It left the tile `key` names. */
  leave(key: string): void;
  /** The tile was clicked: a hover still waiting is dropped, what shows stays until the click lands. */
  commit(): void;
  /** Back to the real skin now. */
  clear(): void;
}

export class TryOn<R> implements TryOnHandle {
  private pointed: Candidate | null = null;
  /** Bumped by every point, leave and clear: a load that finishes under an older token is dropped. */
  private token = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  /** The pointed tile's load has started (or finished). */
  private started = false;

  /**
   * `load` works a candidate out (null: nothing to show) and `show` puts it on the stage (null: the real skin). The
   * newest pair given to `wire` is the one used, so a screen can hand over fresh ones every render.
   */
  constructor(
    private load: (candidate: Candidate) => Promise<R | null>,
    private show: (shown: { candidate: Candidate; result: R } | null) => void,
    private readonly delay = TRY_DELAY_MS,
  ) {}

  wire(load: (candidate: Candidate) => Promise<R | null>, show: (shown: { candidate: Candidate; result: R } | null) => void) {
    this.load = load;
    this.show = show;
  }

  private wait(then: () => void) {
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      then();
    }, this.delay);
  }

  private stop() {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    this.token += 1;
  }

  private run(candidate: Candidate) {
    const mine = this.token;
    this.started = true;
    const land = (result: R | null) => {
      if (mine === this.token) this.show(result === null ? null : { candidate, result });
    };
    this.load(candidate).then(land, () => land(null));
  }

  point(candidate: Candidate) {
    if (this.pointed?.key === candidate.key && (this.timer || this.started)) return;
    // What shows stays until this one is ready, so moving from tile to tile never flashes the real skin.
    this.stop();
    this.pointed = candidate;
    this.started = false;
    this.wait(() => this.run(candidate));
  }

  leave(key: string) {
    if (this.pointed?.key !== key) return;
    this.stop();
    this.pointed = null;
    this.started = false;
    this.wait(() => this.show(null));
  }

  commit() {
    if (!this.pointed || this.started) return;
    this.stop();
  }

  clear() {
    this.stop();
    this.pointed = null;
    this.started = false;
    this.show(null);
  }

  /** The skin changed: the pointed tile is tried again on what is there now. */
  refresh() {
    if (!this.pointed) return;
    this.stop();
    this.run(this.pointed);
  }

  /** Stops what is waiting or loading without showing anything (the screen is going away). */
  dispose() {
    this.stop();
    this.pointed = null;
    this.started = false;
  }
}

/**
 * The handlers a tile needs to be tried on: the pointer over it (never a finger) or keyboard focus on it points, and
 * leaving or losing focus leaves. A click's own focus doesn't point: the click wears the piece.
 */
export function tryHandlers(tryOn: TryOnHandle | undefined, candidate: Candidate) {
  if (!tryOn) return {};
  return {
    onPointerEnter: (event: PointerEvent<HTMLElement>) => {
      if (hoverPreviews(event.pointerType)) tryOn.point(candidate);
    },
    onPointerLeave: () => tryOn.leave(candidate.key),
    onFocus: (event: FocusEvent<HTMLElement>) => {
      if (keyboardFocus(event.currentTarget)) tryOn.point(candidate);
    },
    onBlur: () => tryOn.leave(candidate.key),
  };
}

function keyboardFocus(element: HTMLElement): boolean {
  try {
    return element.matches(':focus-visible');
  } catch {
    return true;
  }
}
