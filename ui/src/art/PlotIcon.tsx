import { useMemo } from 'react';
import { useArtPalette } from './palette';
import { plotCells } from './plot';

export function PlotIcon({ seed, size = 44 }: { seed: string; size?: number }) {
  const palette = useArtPalette();
  const cells = useMemo(() => plotCells(seed, palette), [seed, palette]);
  return (
    <svg viewBox="0 0 8 8" width={size} height={size} shapeRendering="crispEdges" aria-hidden="true">
      {cells.map((c) => (
        <rect key={`${c.x}-${c.y}`} x={c.x} y={c.y} width={1} height={1} fill={c.fill} />
      ))}
    </svg>
  );
}
