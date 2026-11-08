// The skin being edited: its texels, the stroke in progress, undo/redo, and an autosave shortly after each stroke.
// Leaving a skin that was changed keeps a version of it (see the version history spec).
// A skin built from catalog pieces also has a look: the pieces and the painting under them. Its texels are then the
// finished skin, compose(painting, pieces), and it saves through skin_look_save; one without a look works as before.
import { useEffect, useRef, useState } from 'react';
import { commands } from '../../ipc/commands';
import type { SkinLook, SkinModel } from '../../ipc/schema';
import { pushError } from '../../state/toasts';
import { decodeTexture, encodeTexture } from './codec';
import { convertArms, fitsModel } from './armModel';
import { History, mergeStroke, type LookState, type ModelSwap } from './history';
import { mirrorTexel } from './mirror';
import { applyTool, line, Pixels, sameColor, type Change, type Rgba, type Tool, type Touched } from './paint';
import { SKIN_SIZE } from './layout';

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
  /** Shows `data` (already saved, e.g. a restored version) as one undoable step; a restored skin has no look. */
  replace(data: Uint8ClampedArray): void;
  /** The open skin's look, null for a plain painted skin. */
  look: LookState | null;
  /**
   * The open skin's look and texels as they are now, not as of the last render (null while `id` isn't open).
   * `version` changes with every step that changes them (a stroke, undo, redo, a look change, a restore, an arm switch).
   */
  current(id: string): { look: LookState | null; pixels: Pixels; version: number } | null;
  /**
   * Gives the skin a new look (null: plain) whose finished texels are `composed`, as one undo step. `key` joins quick
   * changes of one thing (a colour being dragged) into one step; `model` switches the arms in the same step.
   */
  changeLook(next: LookState | null, composed: Uint8ClampedArray, opts?: { key?: string; model?: ModelSwap }): void;
}

/** Everything that changes without a render of its own: the views redraw on `rev`. */
class DocStore {
  id: string | null = null;
  pixels: Pixels | null = null;
  look: LookState | null = null;
  /** Bumped by every change of texels or look that goes through history, so async work can tell it was overtaken. */
  version = 0;
  /** The saved skin has a look, so saving a plain one clears it first. */
  savedLook = false;
  history = new History();
  stroke: Change[] | null = null;
  touched: Touched = new Set();
  last: [number, number] | null = null;
  timer: ReturnType<typeof setTimeout> | null = null;
  /** A save went through since the skin was opened or last kept as a version. */
  dirty = false;
  onSaved: () => void = () => {};
  onState: (id: string, state: SaveState) => void = () => {};

  wire(onSaved: () => void, onState: (id: string, state: SaveState) => void) {
    this.onSaved = onSaved;
    this.onState = onState;
  }

  open(id: string, data: Uint8ClampedArray, look: LookState | null) {
    this.id = id;
    this.pixels = new Pixels(data);
    this.look = look;
    this.savedLook = look !== null;
    this.version += 1;
    this.history.clear();
    this.dirty = false;
  }

  /** Writes a waiting save, then keeps a version if the skin was changed while it was open. */
  async leave() {
    const id = this.id;
    await this.saveNow();
    if (!id || !this.dirty || this.id !== id) return;
    this.dirty = false;
    try {
      await commands.skinCheckpoint(id);
    } catch (error) {
      pushError(error);
    }
  }

  /** Swaps in `data` as one stroke, so Undo takes it back (and autosaves the texture it replaced). */
  replace(data: Uint8ClampedArray): boolean {
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
    if (changes.length === 0 && !this.look) return false;
    for (const c of changes) px.set(c.x, c.y, c.after);
    // The texture was saved as a plain skin (a restore drops the look), so leaving the look is part of the step.
    const look = this.look ? { before: this.look, after: null } : undefined;
    this.look = null;
    this.savedLook = false;
    this.history.push(changes, undefined, look);
    this.version += 1;
    return true;
  }

  /** Swaps in a new look and its finished texels as one step; true when anything changed. */
  changeLook(next: LookState | null, composed: Uint8ClampedArray, opts: { key?: string; model?: ModelSwap }): boolean {
    const px = this.pixels;
    if (!px) return false;
    const after = new Pixels(composed);
    const changes: Change[] = [];
    for (let y = 0; y < SKIN_SIZE; y++) {
      for (let x = 0; x < SKIN_SIZE; x++) {
        const before = px.get(x, y);
        const now = after.get(x, y);
        if (!sameColor(before, now)) changes.push({ x, y, before, after: now });
      }
    }
    for (const c of changes) px.set(c.x, c.y, c.after);
    this.history.pushLook(changes, { before: this.look, after: next }, opts);
    this.version += 1;
    this.look = next;
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
    this.version += 1;
    return true;
  }

  undo(onModel: (model: SkinModel) => void): boolean {
    const done = !!this.pixels && this.history.undo(this.pixels, { onModel, onLook: (look) => (this.look = look) });
    if (done) this.version += 1;
    return done;
  }

  redo(onModel: (model: SkinModel) => void): boolean {
    const done = !!this.pixels && this.history.redo(this.pixels, { onModel, onLook: (look) => (this.look = look) });
    if (done) this.version += 1;
    return done;
  }

  /** Switches the arms from `from` to `to`; true when texels changed. */
  switchModel(from: SkinModel, to: SkinModel): boolean {
    const px = this.pixels;
    if (!px || from === to) return false;
    const changes = fitsModel(px, to) ? [] : convertArms(px, from, to);
    this.history.push(changes, { before: from, after: to });
    this.version += 1;
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
    const look = this.look;
    if (!id || !px) return;
    this.onState(id, 'saving');
    try {
      const texture = encodeTexture(px.data);
      if (look) {
        await commands.skinLookSave(id, look.look, look.painting ? encodeTexture(look.painting) : null, texture);
        if (this.id === id) this.savedLook = true;
      } else {
        if (this.savedLook) {
          await commands.skinLookClear(id);
          if (this.id === id) this.savedLook = false;
        }
        await commands.skinSave(id, texture);
      }
      if (this.id === id) this.dirty = true;
      this.onSaved();
    } catch (error) {
      pushError(error);
    } finally {
      this.onState(id, 'saved');
    }
  }
}

/** The look a skin is opened with: its pieces and the painting under them, read once as it opens. */
async function openLook(id: string, look: SkinLook | undefined): Promise<LookState | null> {
  if (!look) return null;
  const url = look.painting ? await commands.skinPainting(id) : null;
  const painting = url ? await decodeTexture(url) : null;
  return { look: { ...look, painting: painting !== null }, painting };
}

/**
 * `onModel` stores a new arm model for the open skin: after a switch, and when undo or redo crosses one. `look` is the
 * open skin's look as its wardrobe entry has it (read as the skin opens).
 */
export function useSkinDoc(
  id: string | null,
  model: SkinModel,
  onSaved: () => void,
  onModel: (model: SkinModel) => void,
  look?: SkinLook,
): SkinDoc {
  const [store] = useState(() => new DocStore());
  const [rev, setRev] = useState(0);
  const [loaded, setLoaded] = useState<string | null>(null);
  const [saveOf, setSaveOf] = useState<{ id: string; state: SaveState } | null>(null);
  const bump = () => setRev((r) => r + 1);
  const lookRef = useRef(look);
  // Kept current before the open effect below reads it (effects run in order).
  useEffect(() => {
    lookRef.current = look;
  });

  useEffect(() => {
    store.wire(onSaved, (forId, state) =>
      setSaveOf((s) => (state === 'saved' && s?.id === forId && s.state === 'pending' ? s : { id: forId, state })),
    );
  });

  useEffect(() => {
    if (!id) return;
    let live = true;
    Promise.all([commands.skinTexture(id).then(decodeTexture), openLook(id, lookRef.current)])
      .then(([data, opened]) => {
        if (!live) return;
        store.open(id, data, opened);
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
    replace: (data: Uint8ClampedArray) => {
      if (open && store.replace(data)) bump();
    },
    look: open ? store.look : null,
    current: (forId) => (store.id === forId && store.pixels ? { look: store.look, pixels: store.pixels, version: store.version } : null),
    changeLook: (next, composed, opts = {}) => {
      if (!open || !store.changeLook(next, composed, opts)) return;
      if (opts.model) onModel(opts.model.after);
      bump();
      schedule();
    },
  };
}
