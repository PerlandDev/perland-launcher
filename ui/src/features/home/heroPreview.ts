import { createContext, type ReactNode } from 'react';
import type { Instance } from '../../ipc/schema';

/** Draws Home's header for an instance as a still copy (no dialogs, no menus of its own), for "Adjust the
 *  background" to show the picture under the name and Play. Home provides it; without it the preview shows the
 *  marquee alone. A context, because Home's header itself opens the dialog that shows the copy. */
export const HeroPreview = createContext<((instance: Instance) => ReactNode) | null>(null);
