import { useEffect, useState } from 'react';

/** A server icon is 64 × 64. Its CSS size is whatever gives a whole number of device pixels per icon
 *  pixel, at least 2 per pixel where the screen allows: 128 CSS px at 1, 1.5 or 2 dppx, 102.4 at 1.25
 *  (Windows' 125%), so `image-rendering: pixelated` never draws uneven pixels. */
export function serverIconSize(dpr: number): number {
  const k = Math.max(1, Math.floor(2 * dpr));
  return (64 * k) / dpr;
}

const currentDpr = () => (typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1);

/** `serverIconSize` for this screen, recomputed when the window moves to a screen with another scale
 *  or the OS scale changes. */
export function useServerIconSize(): number {
  const [dpr, setDpr] = useState(currentDpr);
  useEffect(() => {
    if (typeof window.matchMedia !== 'function') return;
    const query = window.matchMedia(`(resolution: ${dpr}dppx)`);
    const onChange = () => setDpr(currentDpr());
    query.addEventListener('change', onChange);
    return () => query.removeEventListener('change', onChange);
  }, [dpr]);
  return serverIconSize(dpr);
}
