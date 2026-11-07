// Undo and redo, one step at a time: a stroke, or a switch of arm model with the texels it redrew.
import type { SkinModel } from '../../ipc/schema';
import type { Change, Pixels } from './paint';

export const HISTORY_CAP = 200;

/** A switch of arm model that a step makes. */
export interface ModelSwap {
  before: SkinModel;
  after: SkinModel;
}

interface Step {
  changes: Change[];
  model?: ModelSwap;
}

export class History {
  private done: Step[] = [];
  private undone: Step[] = [];

  /** Records a finished step. A new step clears what was undone. */
  push(changes: Change[], model?: ModelSwap) {
    if (changes.length === 0 && !model) return;
    this.done.push({ changes, model });
    if (this.done.length > HISTORY_CAP) this.done.shift();
    this.undone = [];
  }

  get canUndo() {
    return this.done.length > 0;
  }

  get canRedo() {
    return this.undone.length > 0;
  }

  /** Undoes the last step; `onModel` hears the arm model to go back to, when the step switched it. */
  undo(px: Pixels, onModel?: (model: SkinModel) => void): boolean {
    const step = this.done.pop();
    if (!step) return false;
    for (const c of [...step.changes].reverse()) px.set(c.x, c.y, c.before);
    if (step.model) onModel?.(step.model.before);
    this.undone.push(step);
    return true;
  }

  redo(px: Pixels, onModel?: (model: SkinModel) => void): boolean {
    const step = this.undone.pop();
    if (!step) return false;
    for (const c of step.changes) px.set(c.x, c.y, c.after);
    if (step.model) onModel?.(step.model.after);
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
