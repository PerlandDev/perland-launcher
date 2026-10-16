import { plotCells } from './plot';

/** What a name's monogram is made of: a letter or digit, with any marks that sit on it. */
const COUNTS = /[\p{L}\p{N}]/u;

/** A word's user-perceived characters, so "ñ" with a combining tilde stays one. */
export function graphemes(word: string): string[] {
  if (typeof Intl.Segmenter !== 'function') return Array.from(word);
  return Array.from(new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(word), (s) => s.segment);
}

/** Up to two initials for an instance's generated icon: the first letter or digit of each of the
 *  name's first two words that have one ("Asalith Fields" is AF, "1.21 Survival" is 1S), upper-cased.
 *  Words split on spaces, '-' and '_'. Empty when the name has no letter or digit at all. */
export function initials(name: string): string {
  const firsts: string[] = [];
  for (const word of name.split(/[\s\-_]+/u)) {
    const first = graphemes(word).find((g) => COUNTS.test(g));
    if (first !== undefined) firsts.push(first);
    if (firsts.length === 2) break;
  }
  return firsts.join('').toLocaleUpperCase();
}

/** The monogram's quiet base: the seed's own plot icon terraces, flattened so the letters keep the
 *  top. Each of the 8 columns is 5px wide on the 40px icon, 8 to 14px tall (a 4px grass top over dirt),
 *  so an instance keeps the slope and the steps its plot icon always had. */
export function terraceHeights(seed: string): number[] {
  const heights = Array<number>(8).fill(0);
  for (const c of plotCells(seed)) heights[c.x] = Math.max(heights[c.x] ?? 0, 8 - c.y);
  return heights.map((h) => Math.min(14, Math.max(8, 4 + 2 * h)));
}
