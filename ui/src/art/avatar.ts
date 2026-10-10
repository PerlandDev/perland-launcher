import type { Cell } from './plot';
import { hash, mix, PLOT_HUES, rng } from './random';

/** A blocky, symmetric 8×8 head generated from the account UUID: never a Mojang skin. */
export function avatarCells(uuid: string): Cell[] {
  const random = rng(hash(`avatar:${uuid}`));
  const hue = PLOT_HUES[hash(uuid) % PLOT_HUES.length] ?? PLOT_HUES[0];
  const skin = mix('#e8b48a', '#7a4a2a', random() * 0.6);
  const hair = mix(hue, '#16121a', 0.35);
  const cells: Cell[] = [];
  for (let y = 0; y < 8; y++) {
    for (let x = 0; x < 4; x++) {
      let fill = skin;
      if (y < 2 || (y === 2 && random() < 0.5) || (x === 0 && y < 5 && random() < 0.6)) fill = hair;
      if (y === 4 && x === 2) fill = '#16121a'; // eyes
      if (y === 6 && x === 3) fill = mix(skin, '#16121a', 0.45); // mouth
      cells.push({ x, y, fill }, { x: 7 - x, y, fill });
    }
  }
  return cells;
}
