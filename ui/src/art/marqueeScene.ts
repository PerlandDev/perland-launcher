import { DIRT, GROUND, hash, mix, PLOT_HUES, rng } from './random';

/** The marquee is a 480×270 panel on an 8-unit pixel grid: 60 columns, 33.75 rows. */
export const COLS = 60;
export const CELL = 8;
const HALF = COLS / 2;
/** The warm tube light behind every marquee panel; instances only tint it slightly. */
const LAMP = '#ffd98a';

export interface MarqueeLayer {
  /** The backlit top edge of the silhouette. */
  rim: string;
  body: string;
  /** Height of each column in cells, mirrored so the strip tiles without a seam. */
  heights: number[];
  /** Seconds for one full drift; nearer layers move faster. */
  duration: number;
}

export interface MarqueeScene {
  /** The panel's unlit ground: a warm black, never a navy sky. */
  sky: string;
  /** The backlight's colour, blooming from behind the horizon. */
  glow: string;
  sun: { x: number; y: number; color: string };
  layers: MarqueeLayer[];
}

/** Terraced hills seen as silhouettes against a lamp behind the horizon, seeded by the instance id. */
export function marqueeScene(seed: string): MarqueeScene {
  const random = rng(hash(seed));
  const accent = PLOT_HUES[hash(seed) % PLOT_HUES.length] ?? PLOT_HUES[0];
  const glow = mix(LAMP, accent, 0.15);
  const sun = {
    x: Math.round(COLS * 0.79) + Math.floor(random() * Math.round(COLS * 0.07)),
    y: 5 + Math.floor(random() * 4),
    color: mix(glow, '#fff8e8', 0.7),
  };
  // Far hills catch the most light; the nearest band is the dark cabinet floor the name sits on.
  const bands = [
    { min: 18, max: 23, light: 0.5 },
    { min: 15, max: 19, light: 0.3 },
    { min: 12, max: 15, light: 0.14 },
    { min: 9, max: 12, light: 0 },
  ];
  const layers = bands.map((band, depth) => {
    const half: number[] = [];
    let h = band.min + Math.floor(random() * (band.max - band.min + 1));
    for (let i = 0; i < HALF; i++) {
      // Mostly single steps, with the odd two-cell jump so rims never run flat for long.
      const roll = random();
      if (roll < 0.08) h -= 2;
      else if (roll < 0.36) h -= 1;
      else if (roll > 0.92) h += 2;
      else if (roll > 0.64) h += 1;
      h = Math.max(band.min, Math.min(band.max, h));
      half.push(h);
    }
    // A dip deeper than one cell must span at least two columns, or it reads as a hanging drip.
    const heights = [...half, ...half.slice().reverse()];
    for (let pass = 0; pass < 2; pass++) {
      for (let i = 0; i < heights.length; i++) {
        const floor = Math.min(heights[(i - 1 + heights.length) % heights.length]!, heights[(i + 1) % heights.length]!) - 1;
        if (heights[i]! < floor) heights[i] = floor;
      }
    }
    const near = depth === bands.length - 1;
    const body = near ? mix(GROUND, DIRT, 0.08) : mix(mix(GROUND, DIRT, 0.2), glow, band.light);
    const hue = PLOT_HUES[(hash(seed) + depth) % PLOT_HUES.length] ?? accent;
    return {
      body,
      rim: near ? mix(body, hue, 0.35) : mix(mix(body, hue, 0.7), glow, 0.2),
      heights,
      duration: 180 - depth * 40,
    };
  });
  return { sky: mix(GROUND, glow, 0.04), glow, sun, layers };
}

/** One layer as a path of columns, drawn twice side by side so it can drift and wrap. */
export function layerPath(heights: number[], rows: number, offsetCols: number, rimOnly: boolean): string {
  return heights
    .map((h, i) => {
      const x = (i + offsetCols) * CELL;
      const top = (rows - h) * CELL;
      const height = rimOnly ? CELL : h * CELL - CELL;
      const y = rimOnly ? top : top + CELL;
      return `M${x} ${y}h${CELL}v${height}h-${CELL}z`;
    })
    .join('');
}

/** A pixel-stepped disc of the given radius in cells, centred on (cx, cy), as one path. */
export function discPath(cx: number, cy: number, radius: number): string {
  let d = '';
  for (let row = -radius; row < radius; row++) {
    const y = row + 0.5;
    const half = Math.round(Math.sqrt(radius * radius - y * y));
    if (half > 0) d += `M${(cx - half) * CELL} ${(cy + row) * CELL}h${half * 2 * CELL}v${CELL}h-${half * 2 * CELL}z`;
  }
  return d;
}
