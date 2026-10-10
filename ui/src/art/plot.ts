import { DIRT, hash, PLOT_HUES, rng } from './random';

export interface Cell {
  x: number;
  y: number;
  fill: string;
}

/** A tiny terraced plot like one quarter of the mark: stepped grass on dirt, in the instance's hue. */
export function plotCells(seed: string): Cell[] {
  const random = rng(hash(`plot:${seed}`));
  const hue = PLOT_HUES[hash(seed) % PLOT_HUES.length] ?? PLOT_HUES[0];
  const rising = random() < 0.5;
  const cells: Cell[] = [];
  let height = 2 + Math.floor(random() * 2);
  for (let col = 0; col < 8; col++) {
    const x = rising ? col : 7 - col;
    if (col > 0 && col % 2 === 0 && random() < 0.8) height = Math.min(7, height + 1);
    const top = 8 - height;
    cells.push({ x, y: top, fill: hue });
    for (let y = top + 1; y < 8; y++) cells.push({ x, y, fill: DIRT });
  }
  return cells;
}
