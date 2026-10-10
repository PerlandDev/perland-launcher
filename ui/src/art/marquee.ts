import { DIRT, GROUND, hash, mix, PLOT_HUES, rng } from './random';

/** The marquee is a 480×270 frame on a 12-unit pixel grid: 40 columns, 22.5 rows. */
export const COLS = 40;
export const CELL = 12;
const HALF = COLS / 2;

export interface MarqueeLayer {
  grass: string;
  body: string;
  /** Height of each column in cells, mirrored so the strip tiles without a seam. */
  heights: number[];
  /** Seconds for one full drift; nearer layers move faster. */
  duration: number;
}

export interface MarqueeScene {
  sky: string[];
  sun: { x: number; y: number; color: string };
  layers: MarqueeLayer[];
}

/** A terraced landscape lit from behind, seeded by the instance id. */
export function marqueeScene(seed: string): MarqueeScene {
  const random = rng(hash(seed));
  const accent = PLOT_HUES[hash(seed) % PLOT_HUES.length] ?? PLOT_HUES[0];
  const skyTop = '#1d1630';
  const horizon = mix(skyTop, accent, 0.5);
  const sky = Array.from({ length: 6 }, (_, i) => mix(skyTop, horizon, i / 5));
  const sun = {
    x: 4 + Math.floor(random() * (COLS - 10)),
    y: 4 + Math.floor(random() * 4),
    color: mix(accent, '#fff6d8', 0.6),
  };
  // The nearest band is the dark cabinet floor the instance name sits on.
  const bands = [
    { min: 12, max: 17, hue: 0.25 },
    { min: 9, max: 14, hue: 0.4 },
    { min: 6, max: 10, hue: 0.6 },
    { min: 4, max: 7, hue: 1 },
  ];
  const layers = bands.map((band, depth) => {
    const half: number[] = [];
    let h = band.min + Math.floor(random() * (band.max - band.min + 1));
    for (let i = 0; i < HALF; i++) {
      const roll = random();
      if (roll < 0.3) h -= 1;
      else if (roll > 0.7) h += 1;
      h = Math.max(band.min, Math.min(band.max, h));
      half.push(h);
    }
    const near = depth === bands.length - 1;
    return {
      grass: near ? accent : mix(GROUND, PLOT_HUES[(hash(seed) + depth) % PLOT_HUES.length] ?? accent, band.hue * 0.6),
      body: near ? GROUND : mix(GROUND, DIRT, 0.15 + depth * 0.12),
      heights: [...half, ...half.slice().reverse()],
      duration: 160 - depth * 35,
    };
  });
  return { sky, sun, layers };
}

/** One layer as a path of columns, drawn twice side by side so it can drift and wrap. */
export function layerPath(heights: number[], rows: number, offsetCols: number, grassOnly: boolean): string {
  return heights
    .map((h, i) => {
      const x = (i + offsetCols) * CELL;
      const top = (rows - h) * CELL;
      const height = grassOnly ? CELL : h * CELL - CELL;
      const y = grassOnly ? top : top + CELL;
      return `M${x} ${y}h${CELL}v${height}h-${CELL}z`;
    })
    .join('');
}
