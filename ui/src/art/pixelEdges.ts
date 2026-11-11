import { useEffect, useState } from 'react';

/** How many cells across the pixel edges are. */
const CELLS = 24;

/** The picture shrunk to `cells` cells across (keeping its shape), as a PNG data URL, for the marquee's pixel edges:
 *  blown back up with hard pixels it reads as big blocks of the picture's own colours. Null when there is no 2D
 *  canvas or the picture can't be decoded; the marquee then keeps its dark edges. */
export async function pixelEdges(src: string, cells = CELLS): Promise<string | null> {
  const canvas = document.createElement('canvas');
  const context = canvas.getContext('2d');
  if (!context) return null;
  const picture = new Image();
  const loaded = new Promise<boolean>((resolve) => {
    picture.onload = () => resolve(true);
    picture.onerror = () => resolve(false);
  });
  picture.src = src;
  if (!(await loaded) || picture.naturalWidth <= 0 || picture.naturalHeight <= 0) return null;
  canvas.width = cells;
  canvas.height = Math.max(1, Math.round((cells * picture.naturalHeight) / picture.naturalWidth));
  context.drawImage(picture, 0, 0, canvas.width, canvas.height);
  try {
    return canvas.toDataURL('image/png');
  } catch {
    return null;
  }
}

/** Made once per picture: the same banner always gives the same edges. */
const made = new Map<string, Promise<string | null>>();

/** `pixelEdges` for `src` (null for none), made once and kept. */
export function usePixelEdges(src: string | null): string | null {
  const [edges, setEdges] = useState<{ src: string; url: string | null } | null>(null);
  useEffect(() => {
    if (!src) return;
    let live = true;
    let job = made.get(src);
    if (!job) {
      job = pixelEdges(src);
      made.set(src, job);
    }
    void job.then((url) => {
      if (live) setEdges({ src, url });
    });
    return () => {
      live = false;
    };
  }, [src]);
  return src && edges?.src === src ? edges.url : null;
}
