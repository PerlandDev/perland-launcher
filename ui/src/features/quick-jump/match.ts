// How Go to matches what the player types: by whole words, whatever the case or the accents, so "configuracion" finds
// "Configuración". The start of a word counts more than the inside of one, and the label more than its context (the
// instance or the screen it belongs to).

/** Text as Go to compares it: lower case, without accents. */
export const fold = (text: string) =>
  text
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase();

const words = (text: string) => fold(text).split(/[^\p{L}\p{N}]+/u).filter(Boolean);

export interface Matchable {
  label: string;
  /** Words that find it too without being its name: the instance it belongs to, the screen it is on. */
  context: string[];
  /** A result of the instance the player is on: first among equals. */
  current?: boolean;
}

/** How well `query` names the entry, higher is better; `null` when a word of it is nowhere. An empty query is 0. */
export function score(query: string, entry: Matchable): number | null {
  const tokens = words(query);
  if (tokens.length === 0) return 0;
  const label = words(entry.label);
  const context = entry.context.flatMap(words);
  const labelText = label.join(' ');
  const contextText = context.join(' ');
  let total = 0;
  for (const token of tokens) {
    if (label.some((word) => word.startsWith(token))) total += label[0]!.startsWith(token) ? 4 : 3;
    else if (context.some((word) => word.startsWith(token))) total += 2;
    else if (labelText.includes(token)) total += 1;
    else if (contextText.includes(token)) total += 0.5;
    else return null;
  }
  return total;
}

/** The entries `query` matches, best first: by score, then the current instance's, then in their own order. */
export function rank<T extends Matchable>(entries: readonly T[], query: string): T[] {
  return entries
    .map((entry, order) => ({ entry, order, score: score(query, entry) }))
    .filter((row): row is { entry: T; order: number; score: number } => row.score !== null)
    .sort((a, b) => b.score - a.score || Number(Boolean(b.entry.current)) - Number(Boolean(a.entry.current)) || a.order - b.order)
    .map((row) => row.entry);
}
