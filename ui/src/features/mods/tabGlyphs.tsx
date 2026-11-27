import type { ComponentType, SVGProps } from 'react';
import type { ModsTab } from '../../state/route';
import { Image, Package, Sun } from '../../ui/Icon';

/** Each Mods & packs tab's glyph: the box for mods (as on Home's key), the picture for resource packs and the sun
 *  for shaders (light and shadow). One map, so every place that names a tab draws it the same. */
export const TAB_GLYPHS: Record<ModsTab, ComponentType<SVGProps<SVGSVGElement>>> = {
  mod: Package,
  resource_pack: Image,
  shader: Sun,
};
