// Recolours a catalog piece's grey art: the mask's red channel names the colour area, the art's grey is the shade.
// Must match the Engine's recolour exactly (float64, floor(v + 0.5)).

/** 64 × 64 × 4 texels, rows top first. */
export type PixelBuffer = Uint8ClampedArray;

const AREA_OF: Record<number, number> = { 85: 0, 170: 1, 255: 2 };

export function parseHex(hex: string | undefined): [number, number, number] | null {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex ?? '');
  return m ? [parseInt(m[1]!, 16), parseInt(m[2]!, 16), parseInt(m[3]!, 16)] : null;
}

function shade(c: number, g: number): number {
  let v: number;
  if (g === 128) v = c;
  else if (g > 128) v = c + ((255 - c) * (g - 128)) / 127;
  else v = (c * g) / 128;
  return Math.floor(v + 0.5);
}

export function recolour(art: PixelBuffer, mask: PixelBuffer, colours: readonly (string | undefined)[]): PixelBuffer {
  const out = new Uint8ClampedArray(art);
  for (let i = 0; i + 3 < art.length; i += 4) {
    const area = AREA_OF[mask[i]!];
    if (area === undefined) continue;
    const rgb = parseHex(colours[area]);
    if (!rgb) continue;
    const g = art[i]!;
    out[i] = shade(rgb[0], g);
    out[i + 1] = shade(rgb[1], g);
    out[i + 2] = shade(rgb[2], g);
  }
  return out;
}
