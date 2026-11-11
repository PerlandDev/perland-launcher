// The Create tab's try-on for the Skins screen: the tiles point at a candidate (see `TryOn`), the editor composes it
// off the document, and the stage shows it while the skin is still the one it was made from.
import { useEffect, useState } from 'react';
import { Pixels } from './paint';
import type { PixelBuffer } from './recolour';
import { TryOn, type TryOnHandle } from './tryOn';
import type { LookEditor } from './useLook';

export interface TryShown {
  /** What the Preview badge names. */
  name: string;
  pixels: Pixels;
  /** Counts previews up, so the stage redraws for each. */
  seq: number;
}

interface Tried {
  skin: string;
  version: number;
  pixels: PixelBuffer;
}

/**
 * `skin` is the open skin and `enabled` whether tiles can be tried now (the Create tab, no history preview). `shown`
 * is never out of date: a preview made from an older version of the skin, or for another skin, isn't shown.
 */
export function useTryOn(editor: LookEditor, skin: string | null, enabled: boolean): { shown: TryShown | null; tryOn: TryOnHandle } {
  const [state, setState] = useState<(Tried & { name: string; seq: number; shown: Pixels }) | null>(null);
  const [tryOn] = useState(() => new TryOn<Tried>(() => Promise.resolve(null), () => {}));
  // Wired after every render, so a preview is made with the editor and for the skin as they are when it starts.
  useEffect(() => {
    tryOn.wire(
      async (candidate) => {
        if (!skin) return null;
        const preview = await editor.preview(candidate.change);
        return preview && { skin, ...preview };
      },
      (shown) => setState((was) => (shown ? { ...shown.result, name: shown.candidate.name, seq: (was?.seq ?? 0) + 1, shown: new Pixels(shown.result.pixels) } : null)),
    );
  });
  useEffect(() => () => tryOn.dispose(), [tryOn]);
  // Another skin, or leaving Create: nothing stays pointed.
  useEffect(() => {
    if (!enabled) tryOn.clear();
    return () => tryOn.clear();
  }, [tryOn, skin, enabled]);
  // The skin changed under a pointed tile (the click that wears it, an undo): it is tried again on what is there now.
  const { rev } = editor;
  useEffect(() => tryOn.refresh(), [tryOn, rev]);

  const current = enabled && state && state.skin === skin && state.version === editor.version() ? state : null;
  return { shown: current ? { name: current.name, pixels: current.shown, seq: current.seq } : null, tryOn };
}
