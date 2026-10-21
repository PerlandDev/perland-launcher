/** Medal rules (medals spec §6.1): pure, from what the launcher already holds. The local time played, and the linked player's
 *  stats per Perland server. Every medal but `time` needs the stats. */

export const HOUR = 3600;

export type MedalUnit = 'time' | 'count';

export interface MedalInput {
  /** Every instance's time on this computer, in seconds. */
  localSeconds: number;
  /** The linked player's servers; null without stats (signed out, not linked, or the add-on off). */
  stats: { servers: readonly { playtimeSeconds: number; advancementIds: readonly string[] }[] } | null;
}

export interface Medal {
  id: string;
  kind: 'ladder' | 'single';
  /** 0 while locked; 1–4 is copper, iron, gold, diamond. */
  tier: number;
  /** The tier a single shows once earned (its rim); a ladder's is its own `tier`. */
  earnedTier: number;
  /** A ladder's reading (seconds or a count); a single's is 0 or 1. */
  value: number;
  /** A ladder's next threshold; null at the top tier and for singles. */
  goal: number | null;
  unit: MedalUnit;
  /** False only for the local time ladder, which shows without Perland. */
  needsStats: boolean;
}

type Stats = NonNullable<MedalInput['stats']>;

interface Ladder {
  id: string;
  unit: MedalUnit;
  /** The four tiers' thresholds, rising. */
  tiers: readonly [number, number, number, number];
  read: (input: MedalInput, stats: Stats | null) => number;
}

export const LADDERS: readonly Ladder[] = [
  { id: 'time', unit: 'time', tiers: [HOUR, 10 * HOUR, 100 * HOUR, 500 * HOUR], read: (input) => input.localSeconds },
  {
    id: 'regular',
    unit: 'time',
    tiers: [HOUR, 10 * HOUR, 50 * HOUR, 200 * HOUR],
    read: (_, stats) => Math.max(0, ...(stats?.servers.map((s) => s.playtimeSeconds) ?? [])),
  },
  {
    id: 'achiever',
    unit: 'count',
    tiers: [5, 25, 50, 100],
    read: (_, stats) => new Set(stats?.servers.flatMap((s) => s.advancementIds) ?? []).size,
  },
  {
    id: 'traveler',
    unit: 'count',
    tiers: [1, 2, 3, 5],
    read: (_, stats) => stats?.servers.filter((s) => s.advancementIds.length > 0).length ?? 0,
  },
];

/** Notable advancements, each a medal earned on any server, with the tier its rim shows. */
export const SINGLES: readonly { id: string; advancement: string; tier: number }[] = [
  { id: 'diamonds', advancement: 'minecraft:story/mine_diamond', tier: 2 },
  { id: 'nether', advancement: 'minecraft:story/enter_the_nether', tier: 2 },
  { id: 'end', advancement: 'minecraft:story/enter_the_end', tier: 3 },
  { id: 'elytra', advancement: 'minecraft:end/elytra', tier: 3 },
  { id: 'hero', advancement: 'minecraft:adventure/hero_of_the_village', tier: 3 },
  { id: 'dragon', advancement: 'minecraft:end/kill_dragon', tier: 4 },
  { id: 'wither', advancement: 'minecraft:nether/summon_wither', tier: 4 },
];

/** Every medal, ladders first, in display order. */
export function medals(input: MedalInput): Medal[] {
  const stats = input.stats;
  const ladders = LADDERS.map((ladder): Medal => {
    const value = ladder.read(input, stats);
    const tier = ladder.tiers.filter((threshold) => value >= threshold).length;
    return {
      id: ladder.id,
      kind: 'ladder',
      tier,
      earnedTier: tier,
      value,
      goal: ladder.tiers[tier] ?? null,
      unit: ladder.unit,
      needsStats: ladder.id !== 'time',
    };
  });
  const earned = new Set(stats?.servers.flatMap((s) => s.advancementIds) ?? []);
  const singles = SINGLES.map((single): Medal => {
    const got = earned.has(single.advancement);
    return {
      id: single.id,
      kind: 'single',
      tier: got ? single.tier : 0,
      earnedTier: single.tier,
      value: got ? 1 : 0,
      goal: null,
      unit: 'count',
      needsStats: true,
    };
  });
  return [...ladders, ...singles];
}
