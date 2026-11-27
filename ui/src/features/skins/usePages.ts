// A long grid drawn a page at a time, so hundreds of tiles never mount at once: the first page, then more when the
// grid's end comes near the bottom of what's scrolled into view, and Show more to do the same by hand (and where the
// webview has no IntersectionObserver).
import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';
import { GRID_PAGE } from './catalogSearch';

export interface Pages {
  /** How many of the list to draw. */
  drawn: number;
  /** How many are left undrawn: while any are, draw the end (`end` on it, a Show more key calling `more` in it). */
  left: number;
  more: () => void;
  end: RefObject<HTMLDivElement | null>;
}

/** `showing` names what the list is (a filter, a language): something else showing starts from the first page again.
 *  `scroller` is the selector of the box the grid scrolls in, an ancestor of its end. */
export function usePages(showing: string, total: number, scroller: string): Pages {
  const [page, setPage] = useState({ showing, drawn: GRID_PAGE });
  const drawn = page.showing === showing ? page.drawn : GRID_PAGE;
  const more = useCallback(() => setPage((p) => ({ showing, drawn: (p.showing === showing ? p.drawn : GRID_PAGE) + GRID_PAGE })), [showing]);
  const end = useRef<HTMLDivElement>(null);
  const left = Math.max(0, total - drawn);
  useEffect(() => {
    const el = end.current;
    if (!el || left <= 0 || typeof IntersectionObserver === 'undefined') return;
    // Watched again after every page: an end still in view after one page asks for the next.
    const observer = new IntersectionObserver((entries) => entries.some((entry) => entry.isIntersecting) && more(), { root: el.closest(scroller), rootMargin: '0px 0px 480px 0px' });
    observer.observe(el);
    return () => observer.disconnect();
  }, [left, more, scroller]);
  return { drawn, left, more, end };
}
