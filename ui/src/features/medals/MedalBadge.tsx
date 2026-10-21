import type { ComponentType, SVGProps } from 'react';
import { Clock, Crown, DiamondGem, Feather, Fire, Moon, Server, Shield, Skull, Trophy, Users } from '../../ui/Icon';

/** Each medal's emblem: a pixelarticons glyph on the badge's face. */
const EMBLEMS: Record<string, ComponentType<SVGProps<SVGSVGElement>>> = {
  time: Clock,
  regular: Server,
  achiever: Trophy,
  traveler: Users,
  diamonds: DiamondGem,
  nether: Fire,
  end: Moon,
  elytra: Feather,
  hero: Shield,
  dragon: Crown,
  wither: Skull,
};

/** The badge's art palette per tier (copper, iron, gold, diamond): pictures, not chrome, so the same in every theme, and
 *  never the role hues. `rim` is the outer ring, `face` the disc, `shine` its lit corner; the emblem is laminate black on
 *  the face (at least 5:1 on each). */
const METALS = [
  null,
  { rim: '#7d3f1d', face: '#c77a48', shine: '#e8a77a' },
  { rim: '#5f646c', face: '#b4b9c0', shine: '#e4e7ea' },
  { rim: '#8a5f0c', face: '#e0aa35', shine: '#ffd97a' },
  { rim: '#1d6f6a', face: '#4cc3b8', shine: '#a8eee6' },
] as const;

/** The disc: a 12 × 12 pixel octagon under the ribbon, as each row's first and last column. */
const DISC: readonly [number, number][] = [
  [3, 8], [1, 10], [0, 11], [0, 11], [0, 11], [0, 11], [0, 11], [0, 11], [0, 11], [0, 11], [1, 10], [3, 8],
];
const DISC_X = 2;
const DISC_Y = 4;

/** The ribbon: two straps meeting behind the disc, as [x, y, width] rects. */
const STRAPS: readonly [number, number, number][] = [
  [3, 0, 3], [10, 0, 3], [4, 1, 3], [9, 1, 3], [5, 2, 2], [9, 2, 2], [6, 3, 4],
];

/** Rows of the disc inset by `by` cells on every side, as rects. */
function rows(by: number) {
  return DISC.slice(by, DISC.length - by).map(([from, to], i) => ({ y: DISC_Y + i + by, x: DISC_X + from + by, w: to - from + 1 - 2 * by }));
}

/** A medal: a pixel octagon on a dirt ribbon, its rim and face the tier's metal, with the medal's emblem in the middle. Locked,
 *  it is the room's `ground` sunk in a `line` rim and ribbon with a dim emblem, so it reads as an empty slot waiting to be
 *  filled. Decorative: the medal's name and state are always in the text beside it. */
export function MedalBadge({ id, tier, size = 48 }: { id: string; tier: number; size?: number }) {
  const metal = METALS[tier] ?? null;
  const Emblem = EMBLEMS[id] ?? Trophy;
  const rim = metal?.rim ?? 'var(--color-line)';
  const face = metal?.face ?? 'var(--color-ground)';
  const strap = metal ? '#7a4a2a' : 'var(--color-line)';
  return (
    <span className="medal-badge" data-locked={metal ? undefined : ''} style={{ width: size, height: size }} aria-hidden>
      <svg className="medal-art" viewBox="0 0 16 16" width={size} height={size} shapeRendering="crispEdges">
        {STRAPS.map(([x, y, w]) => (
          <rect key={`s${x}-${y}`} x={x} y={y} width={w} height={1} fill={strap} />
        ))}
        {rows(0).map((r) => (
          <rect key={`r${r.y}`} x={r.x} y={r.y} width={r.w} height={1} fill={rim} />
        ))}
        {rows(1).map((r) => (
          <rect key={`f${r.y}`} x={r.x} y={r.y} width={r.w} height={1} fill={face} />
        ))}
        {metal && (
          <>
            <rect x={DISC_X + 3} y={DISC_Y + 1} width={3} height={1} fill={metal.shine} />
            <rect x={DISC_X + 1} y={DISC_Y + 2} width={1} height={2} fill={metal.shine} />
          </>
        )}
      </svg>
      <Emblem className="medal-emblem" />
    </span>
  );
}
