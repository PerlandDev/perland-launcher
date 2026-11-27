import type { CSSProperties } from 'react';
import './catalog.css';

/** A catalog thumbnail: a box twice as tall as it is wide, drawn by the style `useThumbs` answers and empty (the same
 *  size) while that loads, so nothing moves when it lands. Size it by its width or its height. */
export function CatalogThumb({ style, className }: { style: CSSProperties | null; className?: string }) {
  return <span aria-hidden className={className ? `skin-thumb ${className}` : 'skin-thumb'} style={style ?? undefined} />;
}
