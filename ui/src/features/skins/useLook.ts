// Edits the open skin's look from the Create tab: put a piece on, take it off, recolour an area, apply a catalog look,
// make everything part of the skin. Each change composes the finished skin at once and goes through the skin doc, so
// it is one undo step (with the stroke history) and autosaves like a stroke.
import { useEffect, useRef, useState } from 'react';
import type { CatalogExpression, CatalogLook, CatalogPiece, SkinLook, SkinModel } from '../../ipc/schema';
import { pushError } from '../../state/toasts';
import { convertArms, fitsModel } from './armModel';
import { compose, FACE_SLOTS, type ComposeLayer } from './compose';
import type { LookState } from './history';
import { composeFace } from './face';
import { applyExpression, applyLook, bakeAll, putOn, setColour, takeOff } from './look';
import { Pixels } from './paint';
import { recolour, type PixelBuffer } from './recolour';
import type { Catalog } from './useCatalog';
import type { SkinDoc } from './useSkinDoc';

export interface LookEditor {
  /** What the skin wears; null for a plain painted skin. */
  look: SkinLook | null;
  /** The player's painting under the pieces; null when there is none. */
  painting: PixelBuffer | null;
  putOn(piece: CatalogPiece): void;
  takeOff(pieceId: string): void;
  /** `live` marks a colour still being dragged or typed: quick changes of one area join one undo step. */
  setColour(pieceId: string, areaId: string, hex: string, live?: boolean): void;
  applyLook(look: CatalogLook): void;
  /** Puts on a ready-made face (its eyes, brows and mouth) as one step, keeping the player's colours. */
  applyExpression(expression: CatalogExpression): void;
  /**
   * Changes the whole outfit as one step (Surprise me). `change` gets the look as it is when its turn comes, after
   * every change asked for before it, and the arms; it may run again if the skin changed while the pieces loaded.
   * Handing the same look back changes nothing.
   */
  restyle(change: (look: SkinLook, model: SkinModel) => SkinLook): void;
  /** Makes every worn piece part of the painting: the finished skin stays, no pieces are left. */
  bakeAll(): void;
  /** Switches the arms: the painting is redrawn for them and the pieces use their art for them. */
  switchModel(to: SkinModel): void;
  /**
   * The last change put on or recoloured a face or body piece (body, eyes, brows, mouth, face details, beard) that the
   * painting covers, so nothing changed where it sits. Cleared by the next change that shows.
   */
  paintingCovers: boolean;
  /** Bumps whenever the open skin's texels change (the doc's `rev`): a preview made before is out of date. */
  rev: number;
  /**
   * The face (8 × 8 texels, see `faceOf`) the open skin would have with `change` made to its look. Nothing is
   * changed, queued or saved; only the files of pieces the changed look wears are loaded. Null with no skin or catalog.
   */
  previewFace(change: (look: SkinLook) => SkinLook): Promise<PixelBuffer | null>;
  /** Resolves once every change asked for so far has landed (or been dropped); wait on it before saving or leaving. */
  settled(): Promise<void>;
}

/** A plain skin starts a look with its own texels as the painting, so the player's work stays under the pieces. */
function startOf(look: LookState | null, pixels: Pixels): LookState {
  return look ?? { look: { pieces: [], painting: true }, painting: new Uint8ClampedArray(pixels.data) };
}

/** `pieces` names the pieces the change is about, so a face piece hidden under the painting can be told. */
type Change = (state: LookState) => { next: LookState; key?: string; to?: SkinModel; pieces?: readonly string[] } | null;

/**
 * Whether `piece`'s layer is a face or body piece the painting hides: where the piece draws, the finished skin didn't
 * change (`before` → `after`), though the painting has paint there. False when the change shows or isn't about one.
 */
function hiddenByPainting(piece: string | undefined, layers: readonly ComposeLayer[], painting: PixelBuffer | null, before: PixelBuffer, after: PixelBuffer): boolean {
  if (!piece || !painting) return false;
  const layer = layers.find((l) => l.id === piece);
  if (!layer || !FACE_SLOTS.includes(layer.slot)) return false;
  const drawn = recolour(layer.art, layer.mask, layer.colours);
  let painted = false;
  for (let i = 3; i < drawn.length; i += 4) {
    if (drawn[i] === 0) continue;
    for (let c = i - 3; c <= i; c++) if (before[c] !== after[c]) return false;
    if (painting[i]! > 0) painted = true;
  }
  return painted;
}

export function useLook(id: string | null, model: SkinModel, doc: SkinDoc, catalog: Catalog): LookEditor {
  const latest = useRef({ id, model, doc, catalog });
  useEffect(() => {
    latest.current = { id, model, doc, catalog };
  });
  // Changes run one after another, each on the look the one before left.
  const queue = useRef<Promise<void>>(Promise.resolve());
  // Live colour changes waiting for their turn, by key: a newer one replaces the waiting one instead of queueing.
  const waiting = useRef(new Map<string, { change: Change }>());
  const [covers, setCovers] = useState<{ id: string | null; hidden: boolean }>({ id: null, hidden: false });

  /**
   * Runs `change` on the look as it is when its turn comes, then composes and records what it returns. When the skin
   * changed while the pieces' files loaded (an Undo, a stroke), the change is worked out again on what is there now,
   * so nothing the player did meanwhile is lost or undone behind their back. Without the catalog nothing changes: the
   * pieces couldn't be drawn, and saving would strip them from the finished skin.
   */
  const run = (change: Change, coalesce?: string) => {
    const forId = latest.current.id;
    if (!forId || !latest.current.catalog.manifest) return;
    if (coalesce) {
      const queued = waiting.current.get(coalesce);
      if (queued) {
        queued.change = change;
        return;
      }
    }
    const entry = { change };
    if (coalesce) waiting.current.set(coalesce, entry);
    queue.current = queue.current.then(async () => {
      if (coalesce && waiting.current.get(coalesce) === entry) waiting.current.delete(coalesce);
      try {
        for (;;) {
          const { doc: d, catalog: c, model: m } = latest.current;
          const now = d.current(forId);
          if (!now) return;
          const result = entry.change(startOf(now.look, now.pixels));
          if (!result) return;
          const { next, key, to, pieces: about = [] } = result;
          const drawnFor = to ?? m;
          const layers = await c.layers(next.look, drawnFor);
          const after = latest.current.doc.current(forId);
          // Another skin opened meanwhile: drop the change.
          if (!after) return;
          if (after.version !== now.version) continue;
          const composed = compose(next.painting, layers);
          const hidden = about.some((piece) => hiddenByPainting(piece, layers, next.painting, now.pixels.data, composed));
          const d2 = latest.current.doc;
          // The doc keeps the pieces, so strokes after this change still go under them (only a full set, as below).
          if (layers.length === next.look.pieces.length) d2.setLayers(next.look, drawnFor, layers);
          d2.changeLook(next, composed, { key, model: to && to !== m ? { before: m, after: to } : undefined });
          setCovers({ id: forId, hidden });
          return;
        }
      } catch (error) {
        pushError(error);
      }
    });
  };

  // Loads the open look's pieces for the doc, so a Paint stroke is composed under them. Only a full set is handed
  // over: a piece missing from the catalog would otherwise vanish from the finished skin at the next stroke.
  const shown = doc.look?.look ?? null;
  const ready = doc.piecesReady;
  useEffect(() => {
    if (!id || !shown || ready || !catalog.manifest) return;
    let live = true;
    catalog
      .layers(shown, model)
      .then((layers) => {
        // A piece with no art for these arms leaves the set short: strokes then stay on top until a Create change.
        if (live && layers.length === shown.pieces.length) latest.current.doc.setLayers(shown, model, layers);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [id, shown, ready, model, catalog]);

  const pieces = () => latest.current.catalog.manifest?.pieces ?? [];
  const withLook = (state: LookState, look: SkinLook): LookState => ({ look: { ...look, painting: state.painting !== null }, painting: state.painting });

  const state = doc.look;
  return {
    look: state?.look ?? null,
    painting: state?.painting ?? null,
    putOn: (piece) => run((s) => ({ next: withLook(s, putOn(s.look, piece, pieces())), pieces: [piece.id] })),
    takeOff: (pieceId) => run((s) => (s.look.pieces.some((w) => w.id === pieceId) ? { next: withLook(s, takeOff(s.look, pieceId)) } : null)),
    setColour: (pieceId, areaId, hex, live = false) =>
      run((s) => {
        const worn = s.look.pieces.find((w) => w.id === pieceId);
        if (!worn || worn.colours[areaId]?.toLowerCase() === hex.toLowerCase()) return null;
        return { next: withLook(s, setColour(s.look, pieceId, areaId, hex)), key: live ? `colour:${pieceId}:${areaId}` : undefined, pieces: [pieceId] };
      }, live ? `colour:${pieceId}:${areaId}` : undefined),
    applyLook: (catalogLook) => run((s) => ({ next: withLook(s, applyLook(s.look, catalogLook)) })),
    applyExpression: (expression) =>
      run((s) => {
        const next = applyExpression(s.look, expression, pieces());
        if (next === s.look) return null;
        const had = new Set(s.look.pieces.map((w) => w.id));
        return { next: withLook(s, next), pieces: next.pieces.filter((w) => !had.has(w.id)).map((w) => w.id) };
      }),
    restyle: (change) =>
      run((s) => {
        const next = change(s.look, latest.current.model);
        if (next === s.look) return null;
        const had = new Set(s.look.pieces.map((w) => w.id));
        return { next: withLook(s, next), pieces: next.pieces.filter((w) => !had.has(w.id)).map((w) => w.id) };
      }),
    bakeAll: () => {
      const forId = latest.current.id;
      if (!forId) return;
      // Read when its turn comes, so a change still loading lands first and is kept in the painting.
      queue.current = queue.current.then(() => {
        const d = latest.current.doc;
        const now = d.current(forId);
        if (!now) return;
        const composed = new Uint8ClampedArray(now.pixels.data);
        const baked = bakeAll(now.look?.painting ?? null, composed);
        // The finished skin is unchanged, so nothing waits on the catalog.
        d.changeLook({ look: baked.look, painting: baked.painting }, composed);
        setCovers({ id: forId, hidden: false });
      });
    },
    switchModel: (to) =>
      run((s) => {
        const from = latest.current.model;
        if (to === from) return null;
        let painting = s.painting;
        if (painting) {
          const px = new Pixels(new Uint8ClampedArray(painting));
          if (!fitsModel(px, to)) convertArms(px, from, to);
          painting = px.data;
        }
        return { next: { look: s.look, painting }, to };
      }),
    paintingCovers: covers.hidden && covers.id === id,
    rev: doc.rev,
    previewFace: async (change) => {
      const { id: forId, doc: d, catalog: c, model: m } = latest.current;
      const now = forId ? d.current(forId) : null;
      if (!now || !c.manifest) return null;
      // A plain skin's own texels are the painting its first piece would go around (see `startOf`); only read here.
      const painting = now.look ? now.look.painting : now.pixels.data;
      const look = change(now.look?.look ?? { pieces: [], painting: true });
      return composeFace(painting, await c.layers(look, m));
    },
    settled: async () => {
      // A change can queue another while it runs: wait until the queue stops growing.
      for (;;) {
        const q = queue.current;
        await q.catch(() => {});
        if (q === queue.current) return;
      }
    },
  };
}
