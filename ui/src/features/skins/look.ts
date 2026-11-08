// Edits a skin's look: the catalog pieces it wears and their colours.
import type { CatalogLook, CatalogPiece, SkinLook, WornPiece } from '../../ipc/schema';
import type { Rgba } from './recolour';

/** Wears `piece`, replacing the one in its slot. `catalog` lets worn pieces be matched by slot; without it only the same id is replaced. */
export function putOn(look: SkinLook, piece: CatalogPiece, catalog: readonly CatalogPiece[] = []): SkinLook {
  const slotOf = new Map(catalog.map((p) => [p.id, p.slot]));
  const colours: Record<string, string> = {};
  for (const a of piece.areas) colours[a.id] = a.default;
  const kept = look.pieces.filter((w) => w.id !== piece.id && slotOf.get(w.id) !== piece.slot);
  return { ...look, pieces: [...kept, { id: piece.id, colours }] };
}

export function takeOff(look: SkinLook, pieceId: string): SkinLook {
  return { ...look, pieces: look.pieces.filter((w) => w.id !== pieceId) };
}

export function setColour(look: SkinLook, pieceId: string, areaId: string, hex: string): SkinLook {
  return {
    ...look,
    pieces: look.pieces.map((w): WornPiece => (w.id === pieceId ? { ...w, colours: { ...w.colours, [areaId]: hex } } : w)),
  };
}

export function applyLook(look: SkinLook, catalogLook: CatalogLook): SkinLook {
  return { ...look, pieces: catalogLook.pieces.map((p) => ({ id: p.id, colours: { ...p.colours } })) };
}

/** Turns everything worn into the player's own painting. */
export function bakeAll(_painting: Rgba | null, composed: Rgba): { painting: Rgba; look: SkinLook } {
  return { painting: composed, look: { pieces: [], painting: true } };
}
