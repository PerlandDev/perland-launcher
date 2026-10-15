/** What an instance's generated art grows from: its pinned seed (kept through a rename, or a picked
 *  landscape), otherwise its id. */
export const artSeed = (i: { id: string; artSeed: string | null }): string => i.artSeed ?? i.id;

/** The six landscapes the picker offers: the base seed, then `~2` to `~6`. A picked preset's own
 *  suffix is stripped first, so picking one never changes the six on offer. */
export function presetSeeds(i: { id: string; artSeed: string | null }): string[] {
  const base = artSeed(i).replace(/~\d+$/, '');
  return [base, ...[2, 3, 4, 5, 6].map((n) => `${base}~${n}`)];
}
