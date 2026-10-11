/** Whether art drawn to cover `box` is blown up enough (2× or more) to be pixel art that should keep
 *  hard pixels. Real art that shrinks, or only grows a little, scales smoothly instead: nearest-neighbour
 *  on those makes jagged, uneven pixels. */
export function pixelUpscale(natural: { width: number; height: number }, box: { width: number; height: number }): boolean {
  if (natural.width <= 0 || natural.height <= 0) return false;
  // object-fit: cover scales by the side that has to stretch more.
  return Math.max(box.width / natural.width, box.height / natural.height) >= 2;
}
