import { useMemo } from 'react';
import { CELL, COLS, layerPath, marqueeScene } from './marquee';

const W = COLS * CELL; // 480
const H = 270;
const ROWS = H / CELL; // 22.5

/** The instance's backlit marquee. In attract mode the layers drift in slow parallax. */
export function Marquee({ seed, attract, className }: { seed: string; attract: boolean; className?: string }) {
  const scene = useMemo(() => marqueeScene(seed), [seed]);
  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="xMidYMax slice"
      shapeRendering="crispEdges"
      aria-hidden="true"
      className={className}
    >
      {scene.sky.map((color, i) => (
        <rect key={color + i} x={0} y={(i * H) / scene.sky.length / 1.6} width={W} height={H} fill={color} />
      ))}
      <rect x={scene.sun.x * CELL} y={scene.sun.y * CELL} width={CELL * 3} height={CELL * 3} fill={scene.sun.color} />
      {scene.layers.map((layer, i) => (
        <g key={i} className="marquee-layer" style={{ animationDuration: `${layer.duration}s`, animationPlayState: attract ? 'running' : 'paused' }}>
          <path d={layerPath(layer.heights, ROWS, 0, false) + layerPath(layer.heights, ROWS, COLS, false)} fill={layer.body} />
          <path d={layerPath(layer.heights, ROWS, 0, true) + layerPath(layer.heights, ROWS, COLS, true)} fill={layer.grass} />
        </g>
      ))}
    </svg>
  );
}
