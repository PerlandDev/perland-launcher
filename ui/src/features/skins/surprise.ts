// "Surprise me": a random outfit for the open skin. The body and its colours stay (the player's skin tone is theirs);
// the face, hair, a top, a bottom and shoes are always dressed, the extras only sometimes. Hair, brows and beard share
// one hair colour, and the clothes take their colours from one curated scheme, so the outfit looks designed.
// Pure: the randomness comes from the caller.
import type { CatalogManifest, CatalogPiece, SkinLook, SkinModel, WornPiece } from '../../ipc/schema';
import { SLOTS } from './compose';
import { HAIR_COLOURS, IRIS_COLOURS, OUTFIT_SCHEMES } from './palettes';
import { parseHex } from './recolour';

/** How often an outfit has each extra. Slots not listed here are always dressed. */
const CHANCE: Readonly<Record<string, number>> = { hat: 0.35, glasses: 0.2, neck: 0.15, back: 0.12, hands: 0.15, belt: 0.15, face_detail: 0.15, beard: 0.1 };

/**
 * Which colour of the scheme each area of a piece takes, by slot: the first entry for its first area, and so on.
 * Where a slot has two orders, the outfit picks one (light or dark trousers, a hat that matches or stands out).
 */
const ORDERS: Readonly<Record<string, readonly (readonly number[])[]>> = {
  top: [[0, 1, 2, 3]],
  bottom: [
    [1, 2, 0, 3],
    [3, 1, 2, 0],
  ],
  shoes: [[3, 1, 2, 0]],
  hat: [
    [2, 1, 3, 0],
    [0, 1, 2, 3],
  ],
  neck: [[2, 1, 0, 3]],
  back: [
    [0, 2, 1, 3],
    [3, 2, 1, 0],
  ],
  hands: [[3, 2, 1, 0]],
  belt: [[3, 2, 1, 0]],
};

/** `hex` a quarter of the way to white: the tips of a hair colour. */
function lighter(hex: string): string {
  const rgb = parseHex(hex);
  if (!rgb) return hex;
  return `#${rgb.map((c) => Math.round(c + (255 - c) * 0.25).toString(16).padStart(2, '0')).join('')}`;
}

/**
 * A random outfit for `look`. `random` answers numbers in [0, 1) like `Math.random`; the same numbers give the same
 * outfit. With `model`, only pieces drawn for those arms are picked. Retired pieces are never picked, and everything
 * worn but the body is replaced.
 */
export function surprise(manifest: CatalogManifest, look: SkinLook, random: () => number, model?: SkinModel): SkinLook {
  const byId = new Map(manifest.pieces.map((p) => [p.id, p]));
  const pick = <T>(list: readonly T[]): T => list[Math.min(list.length - 1, Math.floor(random() * list.length))]!;
  const offered = (slot: string) => manifest.pieces.filter((p) => p.slot === slot && !p.retired && (!model || p.files[model]));
  const wornIn = (slot: string) => look.pieces.find((w) => byId.get(w.id)?.slot === slot);
  const colourOf = (w: WornPiece | undefined, area: string) => w && (w.colours[area] ?? byId.get(w.id)?.areas.find((a) => a.id === area)?.default);
  const defaults = (piece: CatalogPiece) => Object.fromEntries(piece.areas.map((a) => [a.id, a.default]));

  // The body is the player's: the worn one as it is, else the plain one.
  const wornBody = wornIn('body');
  const bodies = offered('body');
  const plain = bodies.find((p) => p.id === 'body-plain') ?? bodies[0];
  const body: WornPiece | undefined = wornBody ?? (plain && { id: plain.id, colours: defaults(plain) });
  const skin = colourOf(body, 'skin');

  // One hair colour for hair, brows and beard: the player's own half of the time, when they have one.
  const wornHair = wornIn('hair');
  const mine = colourOf(wornHair, 'main') ?? colourOf(wornIn('brows'), 'main') ?? colourOf(wornIn('beard'), 'main');
  const keep = mine !== undefined && random() < 0.5;
  const hair = keep ? mine : pick(HAIR_COLOURS);
  const tips = (keep && colourOf(wornHair, 'tips')) || lighter(hair);
  const iris = pick(IRIS_COLOURS);
  const scheme = pick(OUTFIT_SCHEMES);

  const dress = (piece: CatalogPiece): WornPiece => {
    const colours = defaults(piece);
    const order = pick(ORDERS[piece.slot] ?? [[]]);
    let extra = 0;
    piece.areas.forEach((area, i) => {
      if (area.id === 'skin') colours[area.id] = skin ?? area.default;
      else if (piece.slot === 'eyes') colours[area.id] = area.id === 'iris' ? iris : area.default;
      else if (piece.slot === 'hair' || piece.slot === 'brows' || piece.slot === 'beard') {
        if (area.id === 'main') colours[area.id] = hair;
        else if (area.id === 'tips') colours[area.id] = tips;
        // A band or a ribbon in the hair is dressed with the clothes: the accent first.
        else if (piece.slot === 'hair') colours[area.id] = scheme[[2, 1, 3][extra++ % 3]!]!;
      } else if (piece.slot === 'glasses') colours[area.id] = i === 0 ? scheme[3] : area.default;
      else if (order.length > 0) colours[area.id] = scheme[order[i % order.length]!]!;
    });
    return { id: piece.id, colours };
  };

  const pieces: WornPiece[] = body ? [body] : [];
  for (const slot of SLOTS) {
    if (slot === 'body') continue;
    const choices = offered(slot);
    const chance = CHANCE[slot];
    // The dice are rolled for an extra even when the catalog has none, so one empty slot doesn't change the rest.
    if (chance !== undefined && random() >= chance) continue;
    if (choices.length > 0) pieces.push(dress(pick(choices)));
  }
  return { ...look, pieces };
}

