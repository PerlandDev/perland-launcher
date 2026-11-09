// The two skins a new wardrobe entry can start from, drawn here so the launcher ships no Mojang texture:
// a plain grey mannequin, and "the Perlander", an original figure in a lime shirt and dirt-brown overalls.
import type { SkinModel } from '../../ipc/schema';
import { boxOf, PARTS, type FaceName, type Part } from './layout';
import { Pixels, type Rgba } from './paint';

type Paint = Rgba | ((col: number, row: number, w: number, h: number) => Rgba);

function rgb(hex: number): Rgba {
  return [(hex >> 16) & 255, (hex >> 8) & 255, hex & 255, 255];
}

function face(px: Pixels, model: SkinModel, part: Part, name: FaceName, paint: Paint, layer: 'base' | 'outer' = 'base') {
  const f = boxOf(model, part, layer).faces.find((x) => x.name === name)!;
  for (let row = 0; row < f.h; row++) {
    for (let col = 0; col < f.w; col++) {
      px.set(f.x + col, f.y + row, typeof paint === 'function' ? paint(col, row, f.w, f.h) : paint);
    }
  }
}

const ALL_FACES: FaceName[] = ['front', 'back', 'right', 'left', 'top', 'bottom'];

/** A plain mannequin: every base face a quiet grey, sides a step darker so the shape reads. */
export function blankSkin(model: SkinModel = 'classic'): Pixels {
  const px = new Pixels();
  for (const part of PARTS) {
    for (const name of ALL_FACES) {
      const side = name === 'right' || name === 'left' || name === 'back';
      face(px, model, part, name, rgb(side ? 0x86817a : 0x9a948c));
    }
  }
  return px;
}

const SKIN = rgb(0xc98e6a);
const SKIN_SHADE = rgb(0xb07a58);
const HAIR = rgb(0x4a2c18);
const HAIR_HI = rgb(0x6b4024);
const EYE_WHITE = rgb(0xf6f1e7);
const EYE = rgb(0x3a6fd1);
const MOUTH = rgb(0x8a4f3a);
const SHIRT = rgb(0x6cab3c);
const SHIRT_SHADE = rgb(0x558a2e);
const OVERALL = rgb(0x7a4a2a);
const OVERALL_SHADE = rgb(0x5e3820);
const BUCKLE = rgb(0xe8b04a);
const BOOT = rgb(0x2e261f);
const LACE = rgb(0x8c7b69);

/** The Perlander: brown hair, a lime shirt, dirt-brown overalls with amber buckles and dark boots. */
export function starterSkin(model: SkinModel = 'classic'): Pixels {
  const px = new Pixels();
  // Head: hair over the top and back, a fringe on the face.
  face(px, model, 'head', 'top', (c, r) => ((c + r) % 3 === 0 ? HAIR_HI : HAIR));
  face(px, model, 'head', 'back', (c, r) => (r < 7 ? ((c + r) % 4 === 0 ? HAIR_HI : HAIR) : SKIN_SHADE));
  face(px, model, 'head', 'bottom', SKIN_SHADE);
  for (const side of ['right', 'left'] as const) {
    face(px, model, 'head', side, (c, r) => (r < 3 || (r < 6 && (side === 'right' ? c < 3 : c > 4)) ? HAIR : SKIN_SHADE));
  }
  face(px, model, 'head', 'front', (c, r) => {
    if (r < 2) return (c + r) % 3 === 0 ? HAIR_HI : HAIR;
    if (r === 2) return c === 0 || c === 7 || c === 3 ? HAIR : SKIN;
    if (r === 3 && (c === 1 || c === 2 || c === 5 || c === 6)) return HAIR;
    if (r === 4) {
      if (c === 1 || c === 6) return EYE_WHITE;
      if (c === 2 || c === 5) return EYE;
    }
    if (r === 5 && (c === 3 || c === 4)) return SKIN_SHADE;
    if (r === 6 && (c === 3 || c === 4)) return MOUTH;
    if (r === 6 && (c === 2 || c === 5)) return SKIN_SHADE;
    return SKIN;
  });
  // Body: a lime shirt under overall straps, the bib below.
  face(px, model, 'body', 'front', (c, r) => {
    if (r === 0 && c >= 2 && c <= 5) return SHIRT_SHADE;
    if (r >= 8 && r <= 9 && c >= 3 && c <= 4) return r === 8 ? OVERALL_SHADE : OVERALL_SHADE;
    if (r === 11 && (c === 3 || c === 4)) return OVERALL_SHADE;
    if (r >= 6) return r === 6 && (c === 1 || c === 6) ? BUCKLE : OVERALL;
    if (c === 1 || c === 6) return r === 5 ? BUCKLE : OVERALL;
    return SHIRT;
  });
  face(px, model, 'body', 'back', (c, r) => (r >= 6 || c === 2 || c === 5 ? OVERALL_SHADE : SHIRT_SHADE));
  face(px, model, 'body', 'right', (_c, r) => (r >= 6 ? OVERALL_SHADE : SHIRT_SHADE));
  face(px, model, 'body', 'left', (_c, r) => (r >= 6 ? OVERALL_SHADE : SHIRT_SHADE));
  face(px, model, 'body', 'top', SHIRT);
  face(px, model, 'body', 'bottom', OVERALL_SHADE);
  // Arms: short sleeves, bare forearms.
  for (const arm of ['rightArm', 'leftArm'] as const) {
    for (const name of ['front', 'back', 'right', 'left'] as const) {
      const shadow = name !== 'front';
      face(px, model, arm, name, (_c, r) => (r < 4 ? (shadow ? SHIRT_SHADE : SHIRT) : shadow ? SKIN_SHADE : SKIN));
    }
    face(px, model, arm, 'top', SHIRT);
    face(px, model, arm, 'bottom', SKIN_SHADE);
  }
  // Legs: overalls down to the boots.
  for (const leg of ['rightLeg', 'leftLeg'] as const) {
    for (const name of ['front', 'back', 'right', 'left'] as const) {
      const shadow = name !== 'front';
      face(px, model, leg, name, (c, r) => {
        if (r === 10 && name === 'front' && (c === 1 || c === 2)) return LACE;
        if (r >= 10) return BOOT;
        if (r === 9) return OVERALL_SHADE;
        return shadow ? OVERALL_SHADE : OVERALL;
      });
    }
    face(px, model, leg, 'top', OVERALL);
    face(px, model, leg, 'bottom', BOOT);
  }
  grain(px);
  return px;
}

/** A texel-scale grain over every painted texel, the hand-painted noise skins are made of. Deterministic. */
function grain(px: Pixels) {
  for (let y = 0; y < 64; y++) {
    for (let x = 0; x < 64; x++) {
      const c = px.get(x, y);
      if (c[3] === 0) continue;
      let h = Math.imul(x * 73856093 ^ y * 19349663, 2654435761) >>> 0;
      h ^= h >>> 15;
      const d = ((h % 1000) / 1000 - 0.5) * 18;
      const clamp = (v: number) => Math.max(0, Math.min(255, Math.round(v + d)));
      px.set(x, y, [clamp(c[0]), clamp(c[1]), clamp(c[2]), 255]);
    }
  }
}
