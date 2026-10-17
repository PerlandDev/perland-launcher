import { useRef, useState } from 'react';
import { pixelUpscale } from '../../art/upscale';
import { useModIcon } from '../../state/queries';
import { Package } from '../../ui/Icon';
import { useOnScreen } from '../../ui/useOnScreen';

/** The slot is 2.5rem: icons are drawn at 40 CSS px. */
const SLOT = 40;

/** A mod's own icon, read from its jar, in the row's notched slot; the package glyph when it has
 *  none, isn't read yet, or fails to decode. Asked for only once the row is on screen, so a long
 *  list opens no jar it doesn't show. */
export function ModIcon({ id, file, version, off = false }: { id: string; file: string | null; version?: number; off?: boolean }) {
  const ref = useRef<HTMLSpanElement>(null);
  const onScreen = useOnScreen(ref);
  // Once seen, stays asked for: scrolling past a row doesn't drop its icon.
  const [seen, setSeen] = useState(false);
  if (onScreen && !seen) setSeen(true);
  const icon = useModIcon(id, file, version, seen);
  const [failed, setFailed] = useState<string | null>(null);
  const [upscaled, setUpscaled] = useState<string | null>(null);
  const src = icon.data && icon.data !== failed ? icon.data : null;
  return (
    <span ref={ref} className="content-icon" data-off={off || undefined} aria-hidden>
      {src ? (
        <img
          src={src}
          alt=""
          width={SLOT}
          height={SLOT}
          draggable={false}
          data-upscale={upscaled === src ? '' : undefined}
          onLoad={(e) => {
            const { naturalWidth: width, naturalHeight: height } = e.currentTarget;
            setUpscaled(pixelUpscale({ width, height }, { width: SLOT, height: SLOT }) ? src : null);
          }}
          onError={() => setFailed(src)}
        />
      ) : (
        <Package />
      )}
    </span>
  );
}
