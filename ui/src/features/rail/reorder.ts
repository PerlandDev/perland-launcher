/** Pure helpers for reordering the rail: by drag and by Alt+arrow. */

/** A copy of `ids` with the item at `from` moved so it ends up at index `to` (clamped to the list).
 *  An out-of-range `from` leaves the order as it was. */
export function moveItem(ids: string[], from: number, to: number): string[] {
  const next = [...ids];
  if (from < 0 || from >= ids.length) return next;
  const [moved] = next.splice(from, 1);
  next.splice(Math.min(Math.max(to, 0), next.length), 0, moved!);
  return next;
}

/** The slot (0..n) a pointer at `y` drops into, given each cabinet's vertical midpoint top to bottom:
 *  above the first midpoint is slot 0, below the last is slot n. */
export function dropIndex(midpoints: number[], y: number): number {
  return midpoints.filter((mid) => mid < y).length;
}
