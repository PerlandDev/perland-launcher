import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { CELL, COLS, discPath, layerPath, marqueeScene } from './marqueeScene';

const W = COLS * CELL; // 480
const H = 270;
const ROWS = H / CELL; // 33.75

/** The instance's backlit marquee: a lamp blooms behind the horizon, the hills stand in silhouette
 *  with lit rims, and a fine grille sits over the panel. In attract mode the hills drift in slow
 *  parallax; `dimmed` snaps the panel down while the game has the player's attention. */
export function Marquee({
  seed,
  attract,
  dimmed = false,
  banner = null,
  className,
}: {
  seed: string;
  attract: boolean;
  dimmed?: boolean;
  /** A pack's own art (a data: URL). It replaces the generated sky and hills; the grille and vignette stay over it. */
  banner?: string | null;
  className?: string;
}) {
  const scene = useMemo(() => marqueeScene(seed), [seed]);
  const id = useId().replace(/:/g, '');
  const frame = useRef<HTMLDivElement>(null);
  // The panel is anchored at the bottom, so a short panel (the drawer open) crops the sky from the top.
  // Keep the lamp the same relative distance below the visible top so it is never cut into a sliver.
  const [visible, setVisible] = useState(H);
  useEffect(() => {
    const el = frame.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry?.contentRect ?? { width: 0, height: 0 };
      if (width > 0 && height > 0) setVisible(Math.min(H, (W * height) / width));
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const sunX = (scene.sun.x + 4) * CELL;
  const fullSunY = (scene.sun.y + 4) * CELL;
  // Never closer to the visible top than its own radius plus a cell: on a short panel it sets behind the hills whole.
  const sunY = Math.max(H - visible + fullSunY * (visible / H), H - visible + 5 * CELL);
  return (
    <div ref={frame} className={`marquee ${className ?? ''}`} data-dimmed={dimmed || undefined} aria-hidden="true">
      {banner && <img src={banner} alt="" className="marquee-banner" draggable={false} />}
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMax slice" className="h-full w-full">
        <defs>
          <radialGradient id={`${id}-bloom`} cx={sunX} cy={sunY} r={W * 0.62} gradientUnits="userSpaceOnUse">
            <stop offset="0" stopColor={scene.glow} stopOpacity="0.7" />
            <stop offset="0.3" stopColor={scene.glow} stopOpacity="0.26" />
            <stop offset="1" stopColor={scene.glow} stopOpacity="0" />
          </radialGradient>
          <radialGradient id={`${id}-vignette`} cx="50%" cy="45%" r="75%">
            <stop offset="0.55" stopColor="#000" stopOpacity="0" />
            <stop offset="1" stopColor="#000" stopOpacity="0.55" />
          </radialGradient>
          <pattern id={`${id}-grille`} width="3" height="3" patternUnits="userSpaceOnUse">
            <rect width="3" height="1" fill="#000" fillOpacity="0.1" />
          </pattern>
        </defs>
        {!banner && <rect width={W} height={H} fill={scene.sky} />}
        {!banner && <rect width={W} height={H} fill={`url(#${id}-bloom)`} />}
        {!banner && (
          <path
            d={discPath(scene.sun.x + 4, scene.sun.y + 4, 4)}
            transform={`translate(0 ${Math.round((sunY - fullSunY) / CELL) * CELL})`}
            fill={scene.sun.color}
            shapeRendering="crispEdges"
          />
        )}
        {!banner &&
          scene.layers.map((layer, i) => (
            <g
              key={i}
              className="marquee-layer"
              shapeRendering="crispEdges"
              style={{ animationDuration: `${layer.duration}s`, animationPlayState: attract ? 'running' : 'paused' }}
            >
              <path d={layerPath(layer.heights, ROWS, 0, false) + layerPath(layer.heights, ROWS, COLS, false)} fill={layer.body} />
              <path d={layerPath(layer.heights, ROWS, 0, true) + layerPath(layer.heights, ROWS, COLS, true)} fill={layer.rim} />
            </g>
          ))}
        <rect width={W} height={H} fill={`url(#${id}-grille)`} />
        <rect width={W} height={H} fill={`url(#${id}-vignette)`} />
      </svg>
    </div>
  );
}
