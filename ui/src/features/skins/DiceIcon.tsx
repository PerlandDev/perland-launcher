// A die showing five, drawn on the icon set's 24px pixel grid (pixelarticons has none): Surprise me.
import type { SVGProps } from 'react';

export function Dice(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width="24" height="24" fill="currentColor" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path d="M6 3h12v2H6V3Zm0 16h12v2H6v-2ZM3 6h2v12H3V6Zm16 0h2v12h-2V6ZM5 5h1v1H5V5Zm13 0h1v1h-1V5ZM5 18h1v1H5v-1Zm13 0h1v1h-1v-1ZM8 8h2v2H8V8Zm6 0h2v2h-2V8Zm-3 3h2v2h-2v-2Zm-3 3h2v2H8v-2Zm6 0h2v2h-2v-2Z" />
    </svg>
  );
}
