import { useMemo } from 'react';
import { graphemes, initials, terraceHeights } from './monogram';
import { PlotIcon } from './PlotIcon';
import { DIRT, GROUND, mix, PLOT_HUES } from './random';
import { hueIndex } from './variant';

/** The plate's outline on the 40px grid, with the rail art's 3px pixel notch at each corner. */
const PLATE = 'M3 0H37V3H40V37H37V40H3V37H0V3H3Z';
const NOTCH = 3;
const GRASS = 4;

/** An instance's generated icon: its initials in Bungee on a laminate plate tinted toward its hue, over
 *  the terraces of its plot icon. The plate is dark so it never reads as a lit button; the hue lives in
 *  the tint and the grass. A name with no letter or digit gets the plain plot icon. */
export function InstanceIcon({ name, seed, size = 44 }: { name: string; seed: string; size?: number }) {
  const letters = initials(name);
  const art = useMemo(() => {
    const hue = PLOT_HUES[hueIndex(seed)] ?? PLOT_HUES[0];
    return { hue, plate: mix(GROUND, hue, 0.28), heights: terraceHeights(seed) };
  }, [seed]);
  if (!letters) return <PlotIcon seed={seed} size={size} />;
  const two = graphemes(letters).length > 1;
  // Bungee's caps are about 0.72em tall: either size sits centred in the 26px above the tallest terrace.
  const fontSize = two ? 18 : 22;
  const baseline = two ? 20 : 21.5;
  const text = { x: 20, textAnchor: 'middle', fontSize, style: { fontFamily: 'var(--font-display)' } } as const;
  return (
    <svg viewBox="0 0 40 40" width={size} height={size} aria-hidden="true">
      <path d={PLATE} fill={art.plate} />
      <g shapeRendering="crispEdges">
        {art.heights.map((h, col) => (
          <g key={col}>
            <rect x={col * 5} y={40 - h} width={5} height={GRASS} fill={art.hue} />
            <rect x={col * 5} y={40 - h + GRASS} width={5} height={h - GRASS - NOTCH} fill={DIRT} />
          </g>
        ))}
        {/* The dirt's foot runs between the bottom notches. */}
        <rect x={NOTCH} y={40 - NOTCH} width={40 - 2 * NOTCH} height={NOTCH} fill={DIRT} />
      </g>
      {/* The signage's hard shade: dirt one pixel down, then ground two, never a blur. */}
      <text {...text} y={baseline + 2} fill={GROUND}>
        {letters}
      </text>
      <text {...text} y={baseline + 1} fill={DIRT}>
        {letters}
      </text>
      <text {...text} y={baseline} style={{ ...text.style, fill: 'var(--color-ink)' }}>
        {letters}
      </text>
    </svg>
  );
}
