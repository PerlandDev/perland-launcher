// The Create tab's categories: which catalog slots each one dresses. Every slot of the catalog (compose's SLOTS)
// belongs to exactly one, so a piece of any slot can be reached.

/** The categories in order, with the slots each holds (a category of several slots shows one group per slot). */
export const CATEGORIES: readonly { id: string; slots: readonly string[] }[] = [
  { id: 'looks', slots: [] },
  { id: 'body', slots: ['body'] },
  { id: 'face', slots: ['eyes', 'brows', 'mouth', 'face_detail', 'beard'] },
  { id: 'hair', slots: ['hair'] },
  { id: 'tops', slots: ['top'] },
  { id: 'bottoms', slots: ['bottom'] },
  { id: 'shoes', slots: ['shoes'] },
  { id: 'hats', slots: ['hat'] },
  { id: 'accessories', slots: ['glasses', 'neck', 'back', 'hands', 'belt'] },
];

/** Pieces worn on the head show the head only, so a pair of eyes isn't a speck on a whole figure. */
export const HEAD_SLOTS: ReadonlySet<string> = new Set(['eyes', 'brows', 'mouth', 'face_detail', 'beard', 'hair', 'hat', 'glasses']);
