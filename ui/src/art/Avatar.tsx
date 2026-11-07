import { useMemo, useState } from 'react';
import { avatarCells } from './avatarCells';

/** The account's picture: `face` (its own skin's face, as a `data:` URL) when there is one, else a face generated from the UUID. */
export function Avatar({ uuid, size = 32, face = null }: { uuid: string; size?: number; face?: string | null }) {
  const [broken, setBroken] = useState<string | null>(null);
  if (face && broken !== face) {
    return (
      <img
        src={face}
        alt=""
        aria-hidden="true"
        width={size}
        height={size}
        draggable={false}
        className="stepped flex-none [image-rendering:pixelated]"
        onError={() => setBroken(face)}
      />
    );
  }
  return <Generated uuid={uuid} size={size} />;
}

function Generated({ uuid, size }: { uuid: string; size: number }) {
  const cells = useMemo(() => avatarCells(uuid), [uuid]);
  return (
    <svg viewBox="0 0 8 8" width={size} height={size} shapeRendering="crispEdges" aria-hidden="true" className="stepped flex-none">
      {cells.map((c) => (
        <rect key={`${c.x}-${c.y}`} x={c.x} y={c.y} width={1} height={1} fill={c.fill} />
      ))}
    </svg>
  );
}
