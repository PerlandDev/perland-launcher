/** Pure helpers for reordering a list (the rail, the skins wardrobe): by drag and by Alt+arrow. */

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

/** What a drag carries: the row from index `from`, the slot (0..n) it would land in, and each row's vertical midpoint
 *  when the drag began, top to bottom. */
export interface Carried {
  from: number;
  slot: number;
  midpoints: number[];
}

/** Where everything stands while a row is carried. The rows between its own place and the place it
 *  would land snap one place toward its own (a snap, never a slide), so the open slot is always where it
 *  will land: a recessed socket, marked by the drop line, under the carried row. `shift` is how far a
 *  row at `order` has moved; `socket` is how far the open slot is from the carried row's own place. */
export function preview(carry: Carried, order: number): { shift: number; socket: number; moved: boolean } {
  const { from, midpoints: mid } = carry;
  const target = carry.slot > from ? carry.slot - 1 : carry.slot;
  const at = (k: number) => mid[k] ?? 0;
  const shift =
    order > from && order <= target ? at(order - 1) - at(order) : order < from && order >= target ? at(order + 1) - at(order) : 0;
  return { shift, socket: at(target) - at(from), moved: target !== from };
}
