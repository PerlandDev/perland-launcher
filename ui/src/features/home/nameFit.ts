/**
 * How wide, in em, the widest word of an instance name is when Home sets it in Bungee. Home caps
 * the name's size so that word fits its column, rather than letting the browser break a word in
 * the middle. Words split at spaces, and a hyphen ends a word because a line may break after it.
 */

// Bungee's advance widths in thousandths of an em, measured in the app's webview. Bungee draws
// lower case as capitals, so a letter's width doesn't depend on its case.
const LETTERS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ';
const LETTER_WIDTHS = [
  730, 725, 628, 746, 654, 618, 708, 759, 605, 688, 746, 695, 849, 753, 737, 682, 737, 743, 650, 656, 746, 730, 831, 737, 705,
  660,
];
const DIGIT_WIDTHS = [727, 601, 639, 637, 709, 647, 693, 627, 711, 693];
const OTHER_WIDTHS: Record<string, number> = { '-': 420, "'": 356, '.': 384, ',': 384, ':': 384, '!': 424, '?': 656, '&': 766, '(': 429, ')': 429 };
// Anything else (other scripts, symbols, emoji) counts as Bungee's widest letter, so it never overflows.
const FALLBACK = 849;
// A little room for rounding and the hard shade.
const MARGIN = 1.02;

function charWidth(ch: string): number {
  // "Í" measures as "I": drop the accent, then look the base letter up.
  const base = ch.normalize('NFD').replace(/\p{M}/gu, '').toUpperCase();
  if (base.length === 1) {
    const letter = LETTERS.indexOf(base);
    if (letter >= 0) return LETTER_WIDTHS[letter]!;
    if (base >= '0' && base <= '9') return DIGIT_WIDTHS[Number(base)]!;
    if (base in OTHER_WIDTHS) return OTHER_WIDTHS[base]!;
  }
  return FALLBACK;
}

export function widestWordEm(name: string): number {
  const words = name.split(/\s+|(?<=-)/);
  const widths = words.map((word) => [...word].reduce((sum, ch) => sum + charWidth(ch), 0));
  // At least one letter's worth, so the size never divides by zero.
  return Math.round(Math.max(FALLBACK, ...widths) * MARGIN) / 1000;
}
