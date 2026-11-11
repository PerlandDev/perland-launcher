// Edits a skin's look: the catalog pieces it wears and their colours.
import type { CatalogExpression, CatalogLook, CatalogPiece, SkinLook, WornPiece } from '../../ipc/schema';
import type { PixelBuffer } from './recolour';

/** Slots whose colours are the character's own (skin, eyes, hair): they follow the player from piece to piece. */
const OWN_SLOTS: ReadonlySet<string> = new Set(['body', 'eyes', 'brows', 'mouth', 'face_detail', 'beard', 'hair']);
/** For a hair slot put on empty, the worn slots whose `main` it starts from, first found first. */
const HAIR_KIN: Readonly<Record<string, readonly string[]>> = { brows: ['hair', 'beard'], beard: ['hair', 'brows'], hair: ['brows', 'beard'] };

const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/**
 * The colours `piece` starts with on `look`. Taking the place of a worn piece of its slot, each area with the same id
 * keeps the colour it had: always for the character's own slots, and for clothes when the player chose it (the old
 * piece's untouched default gives way to the new piece's, so a top put on looks like its thumbnail). On an empty slot,
 * brows, beard and hair start from each other's `main`. Everything else is the area's default.
 */
function startColours(look: SkinLook, piece: CatalogPiece, byId: ReadonlyMap<string, CatalogPiece>): Record<string, string> {
  const wornIn = (slot: string) => look.pieces.find((w) => byId.get(w.id)?.slot === slot);
  const colourOf = (w: WornPiece, areaId: string) => w.colours[areaId] ?? byId.get(w.id)?.areas.find((a) => a.id === areaId)?.default;
  const replaced = wornIn(piece.slot);
  const was = replaced && byId.get(replaced.id);
  const colours: Record<string, string> = {};
  for (const area of piece.areas) {
    colours[area.id] = area.default;
    const before = was?.areas.find((a) => a.id === area.id);
    if (replaced && before) {
      const had = replaced.colours[area.id] ?? before.default;
      if (OWN_SLOTS.has(piece.slot) || !same(had, before.default)) colours[area.id] = had;
    }
  }
  if (!replaced && colours.main !== undefined) {
    for (const slot of HAIR_KIN[piece.slot] ?? []) {
      const kin = wornIn(slot);
      const main = kin && colourOf(kin, 'main');
      if (main) {
        colours.main = main;
        break;
      }
    }
  }
  return colours;
}

/**
 * Wears `piece`, replacing the one in its slot and keeping the player's colours where the two match (see
 * `startColours`). `catalog` is needed to find the slots and areas of the worn pieces.
 */
export function putOn(look: SkinLook, piece: CatalogPiece, catalog: readonly CatalogPiece[]): SkinLook {
  const byId = new Map(catalog.map((p) => [p.id, p]));
  // The piece itself may be missing from `catalog`: it still has a slot.
  byId.set(piece.id, piece);
  const colours = startColours(look, piece, byId);
  const kept = look.pieces.filter((w) => w.id !== piece.id && byId.get(w.id)?.slot !== piece.slot);
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

/**
 * The expression's eyes, brows and mouth pieces, in that order. Null when it can't be offered: the catalog lacks one,
 * one is retired, or one isn't a piece of the slot it is named for.
 */
export function expressionPieces(expression: CatalogExpression, catalog: readonly CatalogPiece[]): CatalogPiece[] | null {
  const named = ([['eyes', expression.eyes], ['brows', expression.brows], ['mouth', expression.mouth]] as const).map(([slot, id]) =>
    catalog.find((p) => p.id === id && p.slot === slot && !p.retired),
  );
  return named.every((p) => p !== undefined) ? named : null;
}

export function wearsExpression(look: SkinLook | null, expression: CatalogExpression): boolean {
  const worn = new Set(look?.pieces.map((w) => w.id));
  return worn.has(expression.eyes) && worn.has(expression.brows) && worn.has(expression.mouth);
}

/**
 * Puts the expression's three pieces on, each keeping the player's colours as `putOn` does. The same look comes back
 * when there is nothing to change: the expression is already worn, or it can't be offered.
 */
export function applyExpression(look: SkinLook, expression: CatalogExpression, catalog: readonly CatalogPiece[]): SkinLook {
  const pieces = expressionPieces(expression, catalog);
  if (!pieces) return look;
  const worn = new Set(look.pieces.map((w) => w.id));
  return pieces.filter((p) => !worn.has(p.id)).reduce((l, p) => putOn(l, p, catalog), look);
}

/** Turns everything worn into the player's own painting. */
export function bakeAll(_painting: PixelBuffer | null, composed: PixelBuffer): { painting: PixelBuffer; look: SkinLook } {
  return { painting: composed, look: { pieces: [], painting: true } };
}
