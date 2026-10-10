import { useMemo } from 'react';
import { avatarCells } from './avatarCells';

export function Avatar({ uuid, size = 32 }: { uuid: string; size?: number }) {
  const cells = useMemo(() => avatarCells(uuid), [uuid]);
  return (
    <svg viewBox="0 0 8 8" width={size} height={size} shapeRendering="crispEdges" aria-hidden="true" className="stepped flex-none">
      {cells.map((c) => (
        <rect key={`${c.x}-${c.y}`} x={c.x} y={c.y} width={1} height={1} fill={c.fill} />
      ))}
    </svg>
  );
}
