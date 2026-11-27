// Undo and redo, one step at a time: a stroke, a switch of arm model with the texels it redrew, or a change of the
// skin's look (the pieces it wears and the painting under them) with the texels it changed on the finished skin.
import type { SkinLook, SkinModel } from '../../ipc/schema';
import type { Change, Pixels } from './paint';

export const HISTORY_CAP = 200;
/** Look changes under one key this close together (dragging through a colour picker) are one step. */
export const MERGE_MS = 1000;

/** A switch of arm model that a step makes. */
export interface ModelSwap {
  before: SkinModel;
  after: SkinModel;
}

/** A skin's look and the painting under its pieces (null: no painting). Never changed in place once recorded. */
export interface LookState {
  look: SkinLook;
  painting: Uint8ClampedArray | null;
}

/** A change of look a step makes; null is a plain painted skin. */
export interface LookSwap {
  before: LookState | null;
  after: LookState | null;
}

interface Step {
  changes: Change[];
  model?: ModelSwap;
  look?: LookSwap;
  /** Look changes that may merge into this step, and when it last did. */
  key?: string;
  at?: number;
}

export interface Restore {
  onModel?: (model: SkinModel) => void;
  onLook?: (look: LookState | null) => void;
}

export class History {
  private done: Step[] = [];
  private undone: Step[] = [];

  /** Records a finished step. A new step clears what was undone. */
  push(changes: Change[], model?: ModelSwap, look?: LookSwap) {
    if (changes.length === 0 && !model && !look) return;
    this.record({ changes, model, look });
  }

  /** Records a look change; one under the same `key` as the last step, soon after it, joins that step instead. */
  pushLook(changes: Change[], look: LookSwap, opts: { model?: ModelSwap; key?: string; now?: number } = {}) {
    const now = opts.now ?? Date.now();
    const last = this.done[this.done.length - 1];
    if (opts.key && last?.key === opts.key && last.look && !opts.model && now - (last.at ?? 0) < MERGE_MS) {
      last.changes = mergeStroke([...last.changes, ...changes]);
      last.look = { before: last.look.before, after: look.after };
      last.at = now;
      this.undone = [];
      return;
    }
    this.record({ changes, model: opts.model, look, key: opts.key, at: now });
  }

  private record(step: Step) {
    this.done.push(step);
    if (this.done.length > HISTORY_CAP) this.done.shift();
    this.undone = [];
  }

  get canUndo() {
    return this.done.length > 0;
  }

  get canRedo() {
    return this.undone.length > 0;
  }

  /** Undoes the last step; `onModel` hears the arm model to go back to, `onLook` the look, when the step changed them. */
  undo(px: Pixels, restore: Restore | ((model: SkinModel) => void) = {}): boolean {
    const on = typeof restore === 'function' ? { onModel: restore } : restore;
    const step = this.done.pop();
    if (!step) return false;
    for (const c of [...step.changes].reverse()) px.set(c.x, c.y, c.before);
    if (step.model) on.onModel?.(step.model.before);
    if (step.look) on.onLook?.(step.look.before);
    this.undone.push(step);
    return true;
  }

  redo(px: Pixels, restore: Restore | ((model: SkinModel) => void) = {}): boolean {
    const on = typeof restore === 'function' ? { onModel: restore } : restore;
    const step = this.undone.pop();
    if (!step) return false;
    for (const c of step.changes) px.set(c.x, c.y, c.after);
    if (step.model) on.onModel?.(step.model.after);
    if (step.look) on.onLook?.(step.look.after);
    // A redone step never merges with what comes next.
    step.key = undefined;
    this.done.push(step);
    return true;
  }

  clear() {
    this.done = [];
    this.undone = [];
  }
}

/** Joins the changes of one drag into a stroke: each texel keeps its first `before` and its last `after`. */
export function mergeStroke(changes: Change[]): Change[] {
  const byTexel = new Map<number, Change>();
  for (const c of changes) {
    const key = c.y * 64 + c.x;
    const known = byTexel.get(key);
    byTexel.set(key, known ? { ...known, after: c.after } : c);
  }
  return [...byTexel.values()];
}
