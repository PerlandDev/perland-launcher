import { useQueries } from '@tanstack/react-query';
import { commands } from '../../ipc/commands';
import type { CrashFix, ProjectDetail } from '../../ipc/schema';
import { qk } from '../../state/queries';

/** The crash fixes' Find projects that Modrinth has as mods, by the project each fix names. A lookup that fails or
 *  finds something else leaves its project out, so no button promises a page that isn't there. */
export function useModrinthMods(fixes: CrashFix[]): Map<string, ProjectDetail> {
  const finds = fixes.flatMap((fix) => (fix.kind === 'find_mod' ? [fix.project] : []));
  const results = useQueries({
    queries: finds.map((project) => ({
      queryKey: qk.browseProject(project),
      queryFn: () => commands.browseProject(project),
      retry: false,
      staleTime: 5 * 60_000,
    })),
  });
  const found = new Map<string, ProjectDetail>();
  finds.forEach((project, i) => {
    const data = results[i]?.data;
    if (data?.kind === 'mod') found.set(project, data);
  });
  return found;
}
