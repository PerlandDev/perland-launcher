// Edits the open skin's look from the Create tab: put a piece on, take it off, recolour an area, apply a catalog look,
// make everything part of the skin. Each change composes the finished skin at once and goes through the skin doc, so
// it is one undo step (with the stroke history) and autosaves like a stroke.
import { useEffect, useRef } from 'react';
import type { CatalogLook, CatalogPiece, SkinLook, SkinModel } from '../../ipc/schema';
import { pushError } from '../../state/toasts';
import { convertArms, fitsModel } from './armModel';
import { compose } from './compose';
import type { LookState } from './history';
import { applyLook, bakeAll, putOn, setColour, takeOff } from './look';
import { Pixels } from './paint';
import type { PixelBuffer } from './recolour';
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
  /** Makes every worn piece part of the painting: the finished skin stays, no pieces are left. */
  bakeAll(): void;
  /** Switches the arms: the painting is redrawn for them and the pieces use their art for them. */
  switchModel(to: SkinModel): void;
}

/** A plain skin starts a look with its own texels as the painting, so the player's work stays under the pieces. */
function startOf(look: LookState | null, pixels: Pixels): LookState {
  return look ?? { look: { pieces: [], painting: true }, painting: new Uint8ClampedArray(pixels.data) };
}

export function useLook(id: string | null, model: SkinModel, doc: SkinDoc, catalog: Catalog): LookEditor {
  const latest = useRef({ id, model, doc, catalog });
  useEffect(() => {
    latest.current = { id, model, doc, catalog };
  });
  // Changes run one after another, each on the look the one before left.
  const queue = useRef<Promise<void>>(Promise.resolve());

  /** Runs `change` on the look as it is when its turn comes, then composes and records what it returns. */
  const run = (change: (state: LookState) => { next: LookState; key?: string; to?: SkinModel } | null) => {
    const forId = latest.current.id;
    if (!forId) return;
    queue.current = queue.current.then(async () => {
      const { doc: d, catalog: c, model: m } = latest.current;
      const now = d.current(forId);
      if (!now) return;
      const result = change(startOf(now.look, now.pixels));
      if (!result) return;
      const { next, key, to } = result;
      try {
        const layers = await c.layers(next.look, to ?? m);
        // The skin may have changed meanwhile (another skin opened): drop the change then.
        if (!latest.current.doc.current(forId)) return;
        latest.current.doc.changeLook(next, compose(next.painting, layers), { key, model: to && to !== m ? { before: m, after: to } : undefined });
      } catch (error) {
        pushError(error);
      }
    });
  };

  const pieces = () => latest.current.catalog.manifest?.pieces ?? [];
  const withLook = (state: LookState, look: SkinLook): LookState => ({ look: { ...look, painting: state.painting !== null }, painting: state.painting });

  const state = doc.look;
  return {
    look: state?.look ?? null,
    painting: state?.painting ?? null,
    putOn: (piece) => run((s) => ({ next: withLook(s, putOn(s.look, piece, pieces())) })),
    takeOff: (pieceId) => run((s) => (s.look.pieces.some((w) => w.id === pieceId) ? { next: withLook(s, takeOff(s.look, pieceId)) } : null)),
    setColour: (pieceId, areaId, hex, live = false) =>
      run((s) => {
        const worn = s.look.pieces.find((w) => w.id === pieceId);
        if (!worn || worn.colours[areaId]?.toLowerCase() === hex.toLowerCase()) return null;
        return { next: withLook(s, setColour(s.look, pieceId, areaId, hex)), key: live ? `colour:${pieceId}:${areaId}` : undefined };
      }),
    applyLook: (catalogLook) => run((s) => ({ next: withLook(s, applyLook(s.look, catalogLook)) })),
    bakeAll: () => {
      const forId = latest.current.id;
      const now = forId ? doc.current(forId) : null;
      if (!now) return;
      const composed = new Uint8ClampedArray(now.pixels.data);
      const baked = bakeAll(now.look?.painting ?? null, composed);
      // The finished skin is unchanged, so nothing waits on the catalog.
      queue.current = queue.current.then(() => latest.current.doc.changeLook({ look: baked.look, painting: baked.painting }, composed));
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
  };
}
