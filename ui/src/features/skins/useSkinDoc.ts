// The skin being edited: its texels, the stroke in progress, undo/redo, and an autosave shortly after each stroke.
// Leaving a skin that was changed keeps a version of it (see the version history spec).
import { useEffect, useState } from 'react';
import { commands } from '../../ipc/commands';
import type { SkinModel } from '../../ipc/schema';
import { pushError } from '../../state/toasts';
import { decodeTexture, encodeTexture } from './codec';
import { convertArms, fitsModel, restoreArms } from './armModel';
import { History, mergeStroke, type ModelSwap } from './history';
import { mirrorTexel } from './mirror';
import { applyTool, line, Pixels, sameColor, type Change, type Rgba, type Tool, type Touched } from './paint';
import { faceAt, SKIN_SIZE } from './layout';

/** The quiet wait after a stroke before the skin is saved. */
export const AUTOSAVE_MS = 600;

export type SaveState = 'saved' | 'pending' | 'saving';

export interface StrokeOptions {
  tool: Tool;
  color: Rgba;
  mirror: boolean;
  darken: boolean;
}

export interface SkinDoc {
  /** Bumps whenever the texels change, so views redraw. */
  rev: number;
  pixels: Pixels | null;
  canUndo: boolean;
  canRedo: boolean;
  save: SaveState;
  begin(): void;
  /** Paints at (x, y); from the 2D view, a drag joins the last texel to this one. */
  paint(x: number, y: number, opts: StrokeOptions, joined: boolean): void;
  end(): void;
  undo(): void;
  redo(): void;
  /** Switches the arm model, redrawing the arms into the new layout unless the skin already fits it; one undo step. */
  switchModel(to: SkinModel): void;
  /** Saves now if a save is waiting (before switching skins, exporting or wearing). */
  flush(): Promise<void>;
  /** Shows `data` (already saved, e.g. a restored version) as one undoable step, with the arm model it switched to. */
  replace(data: Uint8ClampedArray, model?: ModelSwap): void;
}

/** Whether (x, y) is an arm texel, in either arm model's layout. */
function onArm(x: number, y: number): boolean {
  return (['classic', 'slim'] as const).some((model) => {
    const part = faceAt(model, x, y)?.box.part;
    return part === 'rightArm' || part === 'leftArm';
  });
}

/** Everything that changes without a render of its own: the views redraw on `rev`. */
class DocStore {
  id: string | null = null;
  pixels: Pixels | null = null;
  history = new History();
  stroke: Change[] | null = null;
  touched: Touched = new Set();
  last: [number, number] | null = null;
  timer: ReturnType<typeof setTimeout> | null = null;
  /** A save went through since the skin was opened or last kept as a version. */
  dirty = false;
  /** The last switch of arm model that redrew the arms, while switching straight back can still undo it exactly. */
  lastSwitch: { from: SkinModel; to: SkinModel; changes: Change[] } | null = null;
  onSaved: () => void = () => {};
  onState: (id: string, state: SaveState) => void = () => {};

  wire(onSaved: () => void, onState: (id: string, state: SaveState) => void) {
    this.onSaved = onSaved;
    this.onState = onState;
  }

  open(id: string, data: Uint8ClampedArray) {
    this.id = id;
    this.pixels = new Pixels(data);
    this.history.clear();
    this.dirty = false;
    this.lastSwitch = null;
  }

  /** Writes a waiting save, then keeps a version if the skin was changed while it was open. */
  async leave() {
    const id = this.id;
    await this.saveNow();
    if (!id || !this.dirty || this.id !== id) return;
    this.dirty = false;
    try {
      await commands.skinCheckpoint(id);
      this.onSaved();
    } catch (error) {
      pushError(error);
    }
  }

  /** Swaps in `data` as one stroke, so Undo takes it back (and autosaves the texture it replaced), with the arm model
   *  it switched to, if it did. */
  replace(data: Uint8ClampedArray, model?: ModelSwap): boolean {
    const px = this.pixels;
    if (!px) return false;
    const next = new Pixels(data);
    const changes: Change[] = [];
    for (let y = 0; y < SKIN_SIZE; y++) {
      for (let x = 0; x < SKIN_SIZE; x++) {
        const before = px.get(x, y);
        const after = next.get(x, y);
        if (!sameColor(before, after)) changes.push({ x, y, before, after });
      }
    }
    if (changes.length === 0 && !model) return false;
    for (const c of changes) px.set(c.x, c.y, c.after);
    this.history.push(changes, model);
    this.lastSwitch = null;
    return true;
  }

  /** Starts the save timer; false when no skin is open. */
  schedule(): boolean {
    if (!this.id) return false;
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.saveNow(), AUTOSAVE_MS);
    return true;
  }

  begin() {
    this.stroke = [];
    this.touched = new Set();
    this.last = null;
  }

  /** Paints and returns whether anything changed. */
  paint(model: SkinModel, x: number, y: number, opts: StrokeOptions, joined: boolean): boolean {
    const px = this.pixels;
    if (!px) return false;
    if (!this.stroke) this.begin();
    const from = joined && this.last && opts.tool !== 'fill' ? this.last : null;
    const points: [number, number][] = from ? line(from[0], from[1], x, y).slice(1) : [[x, y]];
    this.last = [x, y];
    let changed = false;
    for (const [tx, ty] of points) {
      const changes = applyTool(px, model, opts.tool, tx, ty, opts, mirrorTexel, this.touched);
      if (changes.length) {
        this.stroke!.push(...changes);
        changed = true;
      }
    }
    return changed;
  }

  /** Ends the stroke; true when it changed something (and went into history). */
  end(): boolean {
    const done = this.stroke;
    this.stroke = null;
    this.last = null;
    if (!done || done.length === 0) return false;
    this.history.push(mergeStroke(done));
    if (done.some((c) => onArm(c.x, c.y))) this.lastSwitch = null;
    return true;
  }

  undo(onModel: (model: SkinModel) => void): boolean {
    this.lastSwitch = null;
    return !!this.pixels && this.history.undo(this.pixels, onModel);
  }

  redo(onModel: (model: SkinModel) => void): boolean {
    this.lastSwitch = null;
    return !!this.pixels && this.history.redo(this.pixels, onModel);
  }

  /** Switches the arms from `from` to `to`; true when texels changed. */
  switchModel(from: SkinModel, to: SkinModel): boolean {
    const px = this.pixels;
    if (!px || from === to) return false;
    // Straight back from a switch that redrew the arms, with them untouched since: the very texels come back.
    const last = this.lastSwitch;
    const back = last && last.from === to && last.to === from ? restoreArms(px, last.changes) : null;
    const changes = back ?? (fitsModel(px, to) ? [] : convertArms(px, from, to));
    this.lastSwitch = back || changes.length === 0 ? null : { from, to, changes };
    this.history.push(changes, { before: from, after: to });
    return changes.length > 0;
  }

  async saveNow() {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    } else {
      return;
    }
    const id = this.id;
    const px = this.pixels;
    if (!id || !px) return;
    this.onState(id, 'saving');
    try {
      await commands.skinSave(id, encodeTexture(px.data));
      if (this.id === id) this.dirty = true;
      this.onSaved();
    } catch (error) {
      pushError(error);
    } finally {
      this.onState(id, 'saved');
    }
  }
}

/** `onModel` stores a new arm model for the open skin: after a switch, and when undo or redo crosses one. */
export function useSkinDoc(id: string | null, model: SkinModel, onSaved: () => void, onModel: (model: SkinModel) => void): SkinDoc {
  const [store] = useState(() => new DocStore());
  const [rev, setRev] = useState(0);
  const [loaded, setLoaded] = useState<string | null>(null);
  const [saveOf, setSaveOf] = useState<{ id: string; state: SaveState } | null>(null);
  const bump = () => setRev((r) => r + 1);

  useEffect(() => {
    store.wire(onSaved, (forId, state) =>
      setSaveOf((s) => (state === 'saved' && s?.id === forId && s.state === 'pending' ? s : { id: forId, state })),
    );
  });

  useEffect(() => {
    if (!id) return;
    let live = true;
    commands
      .skinTexture(id)
      .then(decodeTexture)
      .then((data) => {
        if (!live) return;
        store.open(id, data);
        setLoaded(id);
        setRev((r) => r + 1);
      })
      .catch((error) => live && pushError(error));
    return () => {
      live = false;
      // A waiting save belongs to the skin being left: write it before the next one opens, and keep a version.
      void store.leave();
    };
  }, [id, store]);

  const open = id !== null && loaded === id;
  const schedule = () => {
    if (store.schedule()) setSaveOf({ id: store.id!, state: 'pending' });
  };
  const begin = () => store.begin();
  const paint = (x: number, y: number, opts: StrokeOptions, joined: boolean) => {
    if (open && store.paint(model, x, y, opts, joined)) bump();
  };
  const end = () => {
    if (store.end()) {
      bump();
      schedule();
    }
  };
  const undo = () => {
    if (open && store.undo(onModel)) {
      bump();
      schedule();
    }
  };
  const redo = () => {
    if (open && store.redo(onModel)) {
      bump();
      schedule();
    }
  };
  const switchModel = (to: SkinModel) => {
    if (to === model) return;
    if (open) {
      if (store.switchModel(model, to)) schedule();
      bump();
    }
    onModel(to);
  };

  const save: SaveState = saveOf && saveOf.id === id ? saveOf.state : 'saved';
  return {
    rev,
    pixels: open ? store.pixels : null,
    canUndo: open && store.history.canUndo,
    canRedo: open && store.history.canRedo,
    save,
    begin,
    paint,
    end,
    undo,
    redo,
    switchModel,
    flush: () => store.saveNow(),
    replace: (data: Uint8ClampedArray, swap?: ModelSwap) => {
      if (open && store.replace(data, swap)) bump();
    },
  };
}
