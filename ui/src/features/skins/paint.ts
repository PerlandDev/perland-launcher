// The skin's texels and the tools that change them. Pure: every tool returns the texels it changed, so history can undo them.
import type { SkinModel } from '../../ipc/schema';
import { faceAt, SKIN_SIZE } from './layout';

export type Rgba = [number, number, number, number];
export type Tool = 'pencil' | 'eraser' | 'fill' | 'picker' | 'shade' | 'noise';
export const TOOLS: readonly Tool[] = ['pencil', 'eraser', 'fill', 'picker', 'shade', 'noise'];

export interface Change {
  x: number;
  y: number;
  before: Rgba;
  after: Rgba;
}

export const TRANSPARENT: Rgba = [0, 0, 0, 0];

/** 64 × 64 RGBA texels, rows top first. */
export class Pixels {
  readonly data: Uint8ClampedArray;

  constructor(data?: Uint8ClampedArray) {
    this.data = data ? new Uint8ClampedArray(data) : new Uint8ClampedArray(SKIN_SIZE * SKIN_SIZE * 4);
  }

  get(x: number, y: number): Rgba {
    const i = (y * SKIN_SIZE + x) * 4;
    const d = this.data;
    return [d[i], d[i + 1], d[i + 2], d[i + 3]];
  }

  set(x: number, y: number, [r, g, b, a]: Rgba) {
    const i = (y * SKIN_SIZE + x) * 4;
    this.data[i] = r;
    this.data[i + 1] = g;
    this.data[i + 2] = b;
    this.data[i + 3] = a;
  }

  clone(): Pixels {
    return new Pixels(this.data);
  }
}

export function sameColor(a: Rgba, b: Rgba): boolean {
  // Every fully transparent texel is the same nothing, whatever its RGB.
  if (a[3] === 0 && b[3] === 0) return true;
  return a[0] === b[0] && a[1] === b[1] && a[2] === b[2] && a[3] === b[3];
}

export function toHex([r, g, b]: Rgba): string {
  return `#${[r, g, b].map((c) => c.toString(16).padStart(2, '0')).join('')}`;
}

/** `#rrggbb` (or `rrggbb`, or `#rgb`) as an opaque colour; null when it isn't one. */
export function fromHex(hex: string): Rgba | null {
  let h = hex.trim().replace(/^#/, '');
  if (/^[0-9a-f]{3}$/i.test(h)) h = h.replace(/./g, (c) => c + c);
  if (!/^[0-9a-f]{6}$/i.test(h)) return null;
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16), 255];
}

/** Lightness steps for Shade: each press moves a texel 12% of the way to black or white. */
const SHADE_STEP = 0.12;
/** Noise moves a texel's lightness by up to this many levels either way. */
const NOISE_SPREAD = 14;

function clamp(c: number) {
  return Math.max(0, Math.min(255, Math.round(c)));
}

export function shade(c: Rgba, darken: boolean): Rgba {
  if (c[3] === 0) return c;
  const f = (v: number) => (darken ? v * (1 - SHADE_STEP) : v + (255 - v) * SHADE_STEP);
  return [clamp(f(c[0])), clamp(f(c[1])), clamp(f(c[2])), c[3]];
}

export function noise(c: Rgba, rand: () => number): Rgba {
  if (c[3] === 0) return c;
  const d = (rand() * 2 - 1) * NOISE_SPREAD;
  return [clamp(c[0] + d), clamp(c[1] + d), clamp(c[2] + d), c[3]];
}

export interface ToolOptions {
  color: Rgba;
  /** Also paint the mirrored texel (left ↔ right). */
  mirror: boolean;
  /** Shade darkens instead of lightening. */
  darken?: boolean;
  rand?: () => number;
}

/** Texels the stroke already touched, so a drag doesn't shade or noise one texel over and over. */
export type Touched = Set<number>;

/** The texels the tool at (x, y) reaches: the texel and, with mirror on, its mirror; Fill reaches its whole region. */
function targets(px: Pixels, model: SkinModel, tool: Tool, x: number, y: number, mirror: boolean, mirrorOf: MirrorFn): [number, number][] {
  const starts: [number, number][] = [[x, y]];
  if (mirror) {
    const m = mirrorOf(model, x, y);
    if (m && (m[0] !== x || m[1] !== y)) starts.push(m);
  }
  if (tool !== 'fill') return starts;
  const seen = new Set<number>();
  const out: [number, number][] = [];
  for (const [sx, sy] of starts) {
    for (const t of floodRegion(px, model, sx, sy)) {
      const key = t[1] * SKIN_SIZE + t[0];
      if (!seen.has(key)) {
        seen.add(key);
        out.push(t);
      }
    }
  }
  return out;
}

export type MirrorFn = (model: SkinModel, x: number, y: number) => [number, number] | null;

/** The texels 4-connected to (x, y) with its colour, never leaving its face. */
export function floodRegion(px: Pixels, model: SkinModel, x: number, y: number): [number, number][] {
  const at = faceAt(model, x, y);
  if (!at) return [];
  const { face } = at;
  const target = px.get(x, y);
  const seen = new Set<number>();
  const out: [number, number][] = [];
  const stack: [number, number][] = [[x, y]];
  while (stack.length) {
    const [cx, cy] = stack.pop()!;
    if (cx < face.x || cy < face.y || cx >= face.x + face.w || cy >= face.y + face.h) continue;
    const key = cy * SKIN_SIZE + cx;
    if (seen.has(key)) continue;
    seen.add(key);
    if (!sameColor(px.get(cx, cy), target)) continue;
    out.push([cx, cy]);
    stack.push([cx + 1, cy], [cx - 1, cy], [cx, cy + 1], [cx, cy - 1]);
  }
  return out;
}

/**
 * Applies `tool` at texel (x, y) and returns what changed. The picker changes nothing (read it with `pick`).
 * Texels outside every face are left alone, so the skin's unused area stays empty.
 */
export function applyTool(
  px: Pixels,
  model: SkinModel,
  tool: Tool,
  x: number,
  y: number,
  opts: ToolOptions,
  mirrorOf: MirrorFn,
  touched?: Touched,
): Change[] {
  if (tool === 'picker' || !faceAt(model, x, y)) return [];
  const rand = opts.rand ?? Math.random;
  const changes: Change[] = [];
  for (const [tx, ty] of targets(px, model, tool, x, y, opts.mirror, mirrorOf)) {
    const key = ty * SKIN_SIZE + tx;
    if ((tool === 'shade' || tool === 'noise') && touched?.has(key)) continue;
    touched?.add(key);
    const before = px.get(tx, ty);
    let after: Rgba;
    switch (tool) {
      case 'pencil':
      case 'fill':
        after = opts.color;
        break;
      case 'eraser':
        after = TRANSPARENT;
        break;
      case 'shade':
        after = shade(before, opts.darken ?? false);
        break;
      case 'noise':
        after = noise(before, rand);
        break;
    }
    if (sameColor(before, after) && before[3] === after[3]) continue;
    px.set(tx, ty, after);
    changes.push({ x: tx, y: ty, before, after });
  }
  return changes;
}

export function pick(px: Pixels, x: number, y: number): Rgba {
  return px.get(x, y);
}

/** The texels on the line from (x0, y0) to (x1, y1), both ends included, so a fast drag leaves no gaps. */
export function line(x0: number, y0: number, x1: number, y1: number): [number, number][] {
  const out: [number, number][] = [];
  const dx = Math.abs(x1 - x0);
  const dy = -Math.abs(y1 - y0);
  const sx = x0 < x1 ? 1 : -1;
  const sy = y0 < y1 ? 1 : -1;
  let err = dx + dy;
  let x = x0;
  let y = y0;
  for (;;) {
    out.push([x, y]);
    if (x === x1 && y === y1) return out;
    const e2 = 2 * err;
    if (e2 >= dy) {
      err += dy;
      x += sx;
    }
    if (e2 <= dx) {
      err += dx;
      y += sy;
    }
  }
}
