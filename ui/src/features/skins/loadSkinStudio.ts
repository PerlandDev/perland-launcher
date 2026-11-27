import { sharedLoad } from '../../ui/sharedLoad';

/** The skin studio's chunk (three.js stays out of the start bundle). One shared load: the screen's `lazy` and the
 *  early downloads (hovering the Skins tab, the shell's idle moment) all wait on the same promise. */
export const loadSkinStudio = sharedLoad(() => import('./SkinStudio'));

/** Starts the download ahead of the visit. A failure here stays quiet: the screen shows it if the player opens Skins. */
export function prefetchSkinStudio(): void {
  void loadSkinStudio().catch(() => {});
}
