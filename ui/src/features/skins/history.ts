// Undo and redo, one stroke at a time.
import type { Change, Pixels } from './paint';

export const HISTORY_CAP = 200;

export class History {
  private done: Change[][] = [];
  private undone: Change[][] = [];

  /** Records a finished stroke. A new stroke clears what was undone. */
  push(stroke: Change[]) {
    if (stroke.length === 0) return;
    this.done.push(stroke);
    if (this.done.length > HISTORY_CAP) this.done.shift();
    this.undone = [];
  }

  get canUndo() {
    return this.done.length > 0;
  }

  get canRedo() {
    return this.undone.length > 0;
  }

  undo(px: Pixels): boolean {
    const stroke = this.done.pop();
    if (!stroke) return false;
    for (const c of [...stroke].reverse()) px.set(c.x, c.y, c.before);
    this.undone.push(stroke);
    return true;
  }

  redo(px: Pixels): boolean {
    const stroke = this.undone.pop();
    if (!stroke) return false;
    for (const c of stroke) px.set(c.x, c.y, c.after);
    this.done.push(stroke);
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
