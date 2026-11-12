import { useEffect, useRef, useState } from 'react';

/** Focus that follows a change the player made: `focusOn(a, b, …)` asks for the first of those selectors that is on the
 *  page once it has redrawn, so the keyboard lands on what took the place of a control that is gone (a row that was
 *  removed, a step that is done). While `hold` is true (a dialog is still open over the page) nothing moves. */
export function useFocusOn(hold = false) {
  const wanted = useRef<string[] | null>(null);
  const [, redraw] = useState(0);
  useEffect(() => {
    if (hold || !wanted.current) return;
    for (const selector of wanted.current) {
      const target = document.querySelector<HTMLElement>(selector);
      if (!target) continue;
      wanted.current = null;
      target.focus();
      return;
    }
  });
  return (...selectors: string[]) => {
    wanted.current = selectors;
    redraw((n) => n + 1);
  };
}
