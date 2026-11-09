// The recent colours row. A colour already in it stays where it is, so the row never shuffles under the pointer.

/** `list` with `hex` remembered: a new colour goes in front (keeping at most `max`); a known one changes nothing. */
export function remember(list: string[], hex: string, max: number): string[] {
  if (list.includes(hex)) return list;
  return [hex, ...list].slice(0, max);
}
