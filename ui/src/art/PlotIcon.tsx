import { useMemo } from 'react';
import { plotCells } from './plot';

export function PlotIcon({ seed, size = 44 }: { seed: string; size?: number }) {
  const cells = useMemo(() => plotCells(seed), [seed]);
  return (
    <svg viewBox="0 0 8 8" width={size} height={size} shapeRendering="crispEdges" aria-hidden="true">
      {cells.map((c) => (
        <rect key={`${c.x}-${c.y}`} x={c.x} y={c.y} width={1} height={1} fill={c.fill} />
      ))}
    </svg>
  );
}
