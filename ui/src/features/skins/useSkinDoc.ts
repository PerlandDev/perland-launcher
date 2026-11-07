// The skin being edited: its texels, the stroke in progress, undo/redo, and an autosave shortly after each stroke.
import { useEffect, useState } from 'react';
import { commands } from '../../ipc/commands';
import type { SkinModel } from '../../ipc/schema';
import { pushError } from '../../state/toasts';
import { decodeTexture, encodeTexture } from './codec';
import { convertArms, fitsModel } from './armModel';
import { History, mergeStroke } from './history';
import { mirrorTexel } from './mirror';
import { applyTool, line, Pixels, type Change, type Rgba, type Tool, type Touched } from './paint';

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
    return true;
  }

  undo(onModel: (model: SkinModel) => void): boolean {
    return !!this.pixels && this.history.undo(this.pixels, onModel);
  }

  redo(onModel: (model: SkinModel) => void): boolean {
    return !!this.pixels && this.history.redo(this.pixels, onModel);
  }

  /** Switches the arms from `from` to `to`; true when texels changed. */
  switchModel(from: SkinModel, to: SkinModel): boolean {
    const px = this.pixels;
    if (!px || from === to) return false;
    const changes = fitsModel(px, to) ? [] : convertArms(px, from, to);
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
      // A waiting save belongs to the skin being left: write it before the next one opens.
      void store.saveNow();
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
  };
}
