import { describe, expect, it } from 'vitest';
import { HOUR, LADDERS, medals, SINGLES, type Medal, type MedalInput } from './rules';

const server = (playtimeSeconds: number, advancementIds: string[] = []) => ({ playtimeSeconds, advancementIds });
const find = (list: Medal[], id: string) => list.find((m) => m.id === id) as Medal;
const input = (over: Partial<MedalInput> = {}): MedalInput => ({ localSeconds: 0, stats: null, ...over });

describe('medals', () => {
  it('lists the ladders, then the singles, in a fixed order', () => {
    expect(medals(input()).map((m) => m.id)).toEqual([...LADDERS.map((l) => l.id), ...SINGLES.map((s) => s.id)]);
  });

  it('marks only the local time ladder as not needing stats', () => {
    const list = medals(input());
    expect(list.filter((m) => !m.needsStats).map((m) => m.id)).toEqual(['time']);
  });

  describe('the time ladder', () => {
    it('is locked under an hour, with the first goal next', () => {
      const time = find(medals(input({ localSeconds: 59 * 60 })), 'time');
      expect(time).toMatchObject({ tier: 0, value: 59 * 60, goal: HOUR, unit: 'time' });
    });

    it('climbs a tier at each threshold, exactly', () => {
      const tierAt = (s: number) => find(medals(input({ localSeconds: s })), 'time').tier;
      expect(tierAt(HOUR)).toBe(1);
      expect(tierAt(10 * HOUR - 1)).toBe(1);
      expect(tierAt(10 * HOUR)).toBe(2);
      expect(tierAt(100 * HOUR)).toBe(3);
      expect(tierAt(500 * HOUR)).toBe(4);
    });

    it('has no next goal at the top tier', () => {
      expect(find(medals(input({ localSeconds: 900 * HOUR })), 'time')).toMatchObject({ tier: 4, goal: null });
    });

    it('names the next goal between tiers', () => {
      expect(find(medals(input({ localSeconds: 12 * HOUR })), 'time').goal).toBe(100 * HOUR);
    });
  });

  it('times the regular ladder on the best single server, not the sum', () => {
    const stats = { servers: [server(6 * HOUR), server(6 * HOUR)] };
    expect(find(medals(input({ stats })), 'regular')).toMatchObject({ tier: 1, value: 6 * HOUR, goal: 10 * HOUR });
  });

  it('counts distinct achievements over every server', () => {
    const ids = ['minecraft:story/root', 'minecraft:story/mine_stone', 'a:1', 'a:2', 'a:3'];
    const stats = { servers: [server(0, ids), server(0, ids.slice(0, 2))] };
    expect(find(medals(input({ stats })), 'achiever')).toMatchObject({ tier: 1, value: 5, goal: 25, unit: 'count' });
  });

  it('counts the servers with an achievement for the traveller ladder', () => {
    const stats = { servers: [server(10, ['a:1']), server(10, []), server(10, ['a:2'])] };
    expect(find(medals(input({ stats })), 'traveler')).toMatchObject({ tier: 2, value: 2, goal: 3 });
  });

  it('earns a single on any server, at its fixed tier', () => {
    const stats = { servers: [server(0), server(0, ['minecraft:end/kill_dragon'])] };
    const list = medals(input({ stats }));
    expect(find(list, 'dragon')).toMatchObject({ kind: 'single', tier: 4, earnedTier: 4 });
    expect(find(list, 'nether')).toMatchObject({ kind: 'single', tier: 0, earnedTier: 2 });
  });

  it('leaves every stats medal locked at zero without stats', () => {
    const list = medals(input({ localSeconds: 20 * HOUR }));
    for (const m of list.filter((m) => m.needsStats)) expect(m.tier, m.id).toBe(0);
    expect(find(list, 'time').tier).toBe(2);
  });
});
