/** Scrolls a part of the page into view, without easing, and gives it the keyboard focus. */
export function reveal(target: Element | null, block: ScrollLogicalPosition = 'start') {
  if (!(target instanceof HTMLElement)) return;
  // jsdom has no scrollIntoView; the focus still moves.
  target.scrollIntoView?.({ block });
  target.focus({ preventScroll: true });
}
