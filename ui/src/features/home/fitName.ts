import { useLayoutEffect, useRef } from 'react';

/** The smallest the marquee name may shrink to before a word may break after all: 1.125rem, the smallest
 *  Bungee step in the system (a Browse row name). */
const FLOOR_PX = 18;

/** How far to scale the name's type so its longest word fits on one line: `needed` is the width that word
 *  takes at full size (the box's scroll width), `available` the box's width. Below the 1.125rem floor the word
 *  may break (`floor`), since a 64-letter name with no spaces has to fit somewhere. */
export function nameFit({ available, needed, fontSize }: { available: number; needed: number; fontSize: number }) {
  if (available <= 0 || needed <= available) return { scale: 1, floor: false };
  // A pixel to spare: the widths are rounded, and a word a fraction over would still wrap a letter.
  const scale = Math.floor(((available - 1) / needed) * 10000) / 10000;
  const least = fontSize > 0 ? Math.min(1, FLOOR_PX / fontSize) : 0;
  return scale < least ? { scale: least, floor: true } : { scale, floor: false };
}

/** Keeps the marquee name whole: it wraps only between words, and a word too wide for the column shrinks
 *  the type (`--name-fit`) instead of losing its last letters to a new line. It re-fits when the column's
 *  width, the name or the loaded fonts change. */
export function useNameFit<T extends HTMLElement>(name: string) {
  const ref = useRef<T>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const fit = () => {
      // Measure at full size with words kept whole, so an overflowing word shows in the scroll width.
      el.style.removeProperty('--name-fit');
      delete el.dataset.fit;
      const { scale, floor } = nameFit({
        available: el.clientWidth,
        needed: el.scrollWidth,
        fontSize: parseFloat(getComputedStyle(el).fontSize),
      });
      if (scale < 1) el.style.setProperty('--name-fit', String(scale));
      if (floor) el.dataset.fit = 'floor';
    };
    fit();
    // The name fills its row, so its width is the room it has (the column, less the icon plate when one
    // loads) whatever its size. Its height changes as it fits, so only a new width re-fits.
    let width = el.clientWidth;
    const observer = new ResizeObserver(() => {
      if (el.clientWidth === width) return;
      width = el.clientWidth;
      fit();
    });
    observer.observe(el);
    // Bungee can arrive after the first layout, and the fallback face measures differently.
    let live = true;
    void document.fonts?.ready.then(() => live && fit());
    return () => {
      live = false;
      observer.disconnect();
    };
  }, [name]);
  return ref;
}
