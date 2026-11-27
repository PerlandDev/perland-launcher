import type { MedalView, MedalsView, PerlandStats } from './schema';

/** The mock's medals: the same catalogue and thresholds as `launcher_core::medals::rules`, read from what the mock holds. */

const HOUR = 3600;

/** What the mock knows for the medals that need no account. */
export interface MockMedalInput {
  /** Each instance's time played, in seconds. */
  playtime: readonly number[];
  /** Instances on this computer. */
  instances: number;
  stats: PerlandStats | null;
  signedIn: boolean;
}

/** When the mock says a tier was reached: a fixed day, so screenshots and tests read the same. */
export const MOCK_EARNED_AT = '2026-11-07T21:05:00Z';

type Rule =
  | { id: string; group: MedalView['group']; kind: 'ladder'; unit: MedalView['unit']; steps: readonly number[] }
  | { id: string; group: MedalView['group']; kind: 'single'; rim: number };

const ladder = (id: string, group: MedalView['group'], unit: MedalView['unit'], steps: number[]): Rule => ({ id, group, kind: 'ladder', unit, steps });
const single = (id: string, group: MedalView['group'], rim: number): Rule => ({ id, group, kind: 'single', rim });

export const MOCK_CATALOGUE: readonly Rule[] = [
  single('firstGame', 'local', 1),
  ladder('time', 'local', 'time', [HOUR, 10 * HOUR, 100 * HOUR, 500 * HOUR]),
  ladder('instanceTime', 'local', 'time', [5 * HOUR, 25 * HOUR, 100 * HOUR, 300 * HOUR]),
  ladder('collector', 'local', 'count', [2, 5, 15, 30]),
  ladder('modpacks', 'local', 'count', [1, 3, 10, 20]),
  ladder('modder', 'local', 'count', [10, 50, 200, 500]),
  ladder('versions', 'local', 'count', [2, 5, 10, 20]),
  ladder('days', 'local', 'count', [7, 30, 100, 365]),
  ladder('marathon', 'local', 'time', [2 * HOUR, 4 * HOUR, 8 * HOUR, 12 * HOUR]),
  single('theme', 'local', 1),
  single('decorator', 'local', 1),
  single('tailor', 'local', 1),
  single('doctor', 'local', 2),
  single('completionist', 'local', 4),
  ladder('regular', 'perland', 'time', [HOUR, 10 * HOUR, 50 * HOUR, 200 * HOUR]),
  ladder('achiever', 'perland', 'count', [5, 25, 50, 100]),
  ladder('traveler', 'perland', 'count', [1, 2, 3, 5]),
  single('diamonds', 'perland', 2),
  single('nether', 'perland', 2),
  single('end', 'perland', 3),
  single('elytra', 'perland', 3),
  single('hero', 'perland', 3),
  single('dragon', 'perland', 4),
  single('wither', 'perland', 4),
];

const ADVANCEMENTS: Record<string, string> = {
  diamonds: 'minecraft:story/mine_diamond',
  nether: 'minecraft:story/enter_the_nether',
  end: 'minecraft:story/enter_the_end',
  elytra: 'minecraft:end/elytra',
  hero: 'minecraft:adventure/hero_of_the_village',
  dragon: 'minecraft:end/kill_dragon',
  wither: 'minecraft:nether/summon_wither',
};

function reading(id: string, input: MockMedalInput): number {
  const servers = input.stats?.servers ?? [];
  const total = input.playtime.reduce((a, b) => a + b, 0);
  switch (id) {
    case 'firstGame':
      return total > 0 ? 1 : 0;
    case 'time':
      return total;
    case 'instanceTime':
      return Math.max(0, ...input.playtime);
    case 'collector':
      return input.instances;
    case 'regular':
      return Math.max(0, ...servers.map((s) => s.playtimeSeconds));
    case 'achiever':
      return new Set(servers.flatMap((s) => s.advancementIds)).size;
    case 'traveler':
      return servers.filter((s) => s.advancementIds.length > 0).length;
    default: {
      const advancement = ADVANCEMENTS[id];
      return advancement && servers.some((s) => s.advancementIds.includes(advancement)) ? 1 : 0;
    }
  }
}

/** The mock's answer to `medals_view`. */
export function mockMedalsView(input: MockMedalInput): MedalsView {
  const medals = MOCK_CATALOGUE.map((rule): MedalView => {
    const value = reading(rule.id, input);
    if (rule.kind === 'ladder') {
      const tier = rule.steps.filter((s) => value >= s).length;
      return {
        id: rule.id,
        group: rule.group,
        kind: 'ladder',
        unit: rule.unit,
        tier,
        rim: 0,
        value,
        goal: rule.steps[tier] ?? null,
        steps: [...rule.steps],
        earnedAt: tier > 0 ? MOCK_EARNED_AT : null,
      };
    }
    const tier = value > 0 ? rule.rim : 0;
    return { id: rule.id, group: rule.group, kind: 'single', unit: 'count', tier, rim: rule.rim, value, goal: null, steps: [], earnedAt: tier > 0 ? MOCK_EARNED_AT : null };
  });
  return { medals, savedToAccount: input.signedIn };
}
