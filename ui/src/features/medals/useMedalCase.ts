import { usePerlandStats, usePlaytime } from '../../state/queries';
import { medals, type Medal } from './rules';

/** The player's medal case: every medal that can show (only the local time medal while Perland's stats are off), and how
 *  many are earned. The Profile's medals count from here. */
export function useMedalCase(): { list: Medal[]; earned: number; statsOn: boolean } {
  const playtime = usePlaytime();
  const stats = usePerlandStats();
  const statsOn = Boolean(stats.data);
  const localSeconds = playtime.data?.reduce((sum, p) => sum + p.seconds, 0) ?? 0;
  const list = medals({ localSeconds, stats: stats.data ?? null }).filter((m) => statsOn || !m.needsStats);
  return { list, earned: list.filter((m) => m.tier > 0).length, statsOn };
}
