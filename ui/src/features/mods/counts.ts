import type { Instance } from '../../ipc/schema';
import { useContentFiles, useMods, usePackContents } from '../../state/queries';
import type { ModsTab } from '../../state/route';
import { isPack } from '../home/labels';

/** How many mods the Mods tab lists for this instance: on a pack, the pack's own and the player's additions.
 *  `undefined` until it is known, so a count never flashes 0 while it loads. */
export function useModCount(id: string, instance: Instance | undefined): number | undefined {
  const pack = instance ? isPack(instance) : false;
  const mods = useMods(id);
  const contents = usePackContents(id, pack);
  if (!instance) return undefined;
  if (pack) return contents.data ? contents.data.managed.length + contents.data.additions.length : undefined;
  return mods.data?.length;
}

/** What each Mods & packs tab lists, by tab; a tab's count is `undefined` until it is known. */
export function useContentCounts(id: string, instance: Instance | undefined): Record<ModsTab, number | undefined> {
  const resourcePacks = useContentFiles(id, 'resource_pack');
  const shaders = useContentFiles(id, 'shader');
  return {
    mod: useModCount(id, instance),
    resource_pack: resourcePacks.data?.length,
    shader: shaders.data?.length,
  };
}
