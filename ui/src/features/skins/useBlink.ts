// The stage's blink, in two halves: `useBlinkFrame` builds the open skin's closed-eyes frame (see blink.ts) off the
// document, and `useBlinking` says when the eyes are shut. Neither touches the skin.
import { useEffect, useRef, useState } from 'react';
import type { SkinModel } from '../../ipc/schema';
import { BLINK_MS, blinkGap, blinkSeed, closedEyes } from './blink';
import { Pixels } from './paint';
import type { Catalog } from './useCatalog';
import type { SkinDoc } from './useSkinDoc';

/** The skin rests this long after a change before its closed-eyes frame is made: a colour being dragged makes none. */
export const BLINK_SETTLE_MS = 250;

/**
 * True while the eyes are shut: for `BLINK_MS` every 3 to 6 seconds, at the rhythm of `seed` (the skin). Never while
 * `active` is false; turning it off opens the eyes at once and starts the rhythm over. Like the pose, it is the
 * figure's own motion: the Animation switch stops it, the system's reduced-motion setting does not.
 */
export function useBlinking(active: boolean, seed: string): boolean {
  const [shut, setShut] = useState(false);
  useEffect(() => {
    if (!active) return;
    const rhythm = blinkSeed(seed);
    let n = 0;
    let timer: ReturnType<typeof setTimeout>;
    const open = () => {
      setShut(false);
      timer = setTimeout(close, blinkGap(rhythm, n++));
    };
    const close = () => {
      setShut(true);
      timer = setTimeout(open, BLINK_MS);
    };
    timer = setTimeout(close, blinkGap(rhythm, n++));
    return () => {
      clearTimeout(timer);
      setShut(false);
    };
  }, [active, seed]);
  return active && shut;
}

/**
 * The closed-eyes frame of the open skin, for the stage to show during a blink; null while there is nothing to blink
 * (see `closedEyes`), while `enabled` is false, and from any change of the skin until the frame is made again. The
 * pieces' files are the ones the open skin already loaded; a skin with no eyes piece asks for none.
 */
export function useBlinkFrame(skin: string | null, doc: SkinDoc, catalog: Catalog, model: SkinModel, enabled: boolean): Pixels | null {
  const latest = useRef({ doc, catalog });
  useEffect(() => {
    latest.current = { doc, catalog };
  });
  const [frame, setFrame] = useState<{ skin: string; version: number; rev: number; pixels: Pixels } | null>(null);
  const { rev, look } = doc;
  const { manifest } = catalog;
  // Whether the look wears an eyes piece at all: without one there is nothing to work out.
  const eyes = !!look && !!manifest && look.look.pieces.some((w) => manifest.pieces.some((p) => p.id === w.id && p.slot === 'eyes'));
  useEffect(() => {
    if (!enabled || !skin || !eyes) return;
    let live = true;
    const timer = setTimeout(() => {
      const { doc: d, catalog: c } = latest.current;
      const now = d.current(skin);
      if (!now?.look) return;
      const { painting } = now.look;
      c.layers(now.look.look, model)
        .then((layers) => {
          const after = latest.current.doc.current(skin);
          if (!live || !after || after.version !== now.version) return;
          const closed = closedEyes(after.pixels.data, painting, layers);
          setFrame(closed ? { skin, version: now.version, rev, pixels: new Pixels(closed) } : null);
        })
        // The pieces can't be read now: no blink until the skin changes again.
        .catch(() => {});
    }, BLINK_SETTLE_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [enabled, skin, eyes, rev, look, model, manifest]);
  // Out of date the moment the skin redraws (`rev` moves with every stroke texel too) or its version moves on.
  const current = enabled && eyes && frame && frame.skin === skin && frame.rev === rev && frame.version === doc.current(skin)?.version;
  return current ? frame.pixels : null;
}
