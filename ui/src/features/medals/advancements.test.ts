import { createInstance, type TFunction } from 'i18next';
import { beforeAll, describe, expect, it } from 'vitest';
import en from '../../i18n/en.json';
import es from '../../i18n/es.json';
import { advancementName, groupAchievements, groupName, groupOf, isKnown, TABS, VANILLA } from './advancements';

let tEn: TFunction;
let tEs: TFunction;

beforeAll(async () => {
  const make = async (lng: string) => {
    const i18n = createInstance();
    await i18n.init({ resources: { en: { translation: en }, es: { translation: es } }, lng, fallbackLng: 'en' });
    return i18n.t;
  };
  tEn = await make('en');
  tEs = await make('es');
});

describe('the vanilla table', () => {
  it('has a name in English and Spanish for every advancement and tab', () => {
    const names = (lang: typeof en) => (lang as unknown as { advancements: Record<string, Record<string, string>> }).advancements;
    for (const tab of TABS) {
      for (const name of VANILLA[tab]) {
        expect(names(en)[tab]?.[name], `en ${tab}/${name}`).toBeTruthy();
        expect(names(es)[tab]?.[name], `es ${tab}/${name}`).toBeTruthy();
      }
      expect(Object.keys(names(en)[tab] ?? {}).sort()).toEqual([...VANILLA[tab]].sort());
      expect(tEn(`achievements.tabs.${tab}`)).not.toContain('achievements.');
    }
  });

  it('has no duplicates', () => {
    for (const tab of TABS) expect(new Set(VANILLA[tab]).size).toBe(VANILLA[tab].length);
  });
});

describe('advancementName', () => {
  it('translates a vanilla advancement', () => {
    expect(advancementName('minecraft:story/mine_diamond', tEn)).toBe('Diamonds!');
    expect(advancementName('minecraft:story/mine_diamond', tEs)).toBe('¡Diamantes!');
  });

  it('reads a modded or unknown ID', () => {
    expect(advancementName('create:andesite_alloy', tEn)).toBe('Andesite alloy');
    expect(advancementName('create:kinetics/belt-drive', tEn)).toBe('Belt drive');
    expect(advancementName('minecraft:story/some_new_one', tEn)).toBe('Some new one');
    expect(advancementName('not an id', tEn)).toBe('not an id');
  });
});

describe('groupOf and isKnown', () => {
  it('sorts IDs into tabs, mods and other', () => {
    expect(groupOf('minecraft:nether/find_fortress')).toBe('nether');
    expect(groupOf('create:root')).toBe('create');
    expect(groupOf('minecraft:recipes/misc/x')).toBe('other');
    expect(groupOf('minecraft:story')).toBe('other');
    expect(groupOf(':x')).toBe('other');
    expect(isKnown('minecraft:end/kill_dragon')).toBe(true);
    expect(isKnown('minecraft:end/kill_dragon/extra')).toBe(false);
    expect(isKnown('create:root')).toBe(false);
  });

  it('names groups', () => {
    expect(groupName('end', tEn)).toBe('The End');
    expect(groupName('create', tEn)).toBe('create');
    expect(groupName('other', tEs)).toBe('Otros');
  });
});

describe('groupAchievements', () => {
  it('lists tabs in the game order with the locked ones, then mods by name, then other', () => {
    const groups = groupAchievements([
      'minecraft:story/mine_stone',
      'minecraft:story/root',
      'minecraft:end/root',
      'zeta:a',
      'create:root',
      'minecraft:story/mine_stone',
      'weird',
    ]);
    expect(groups.map((g) => g.group)).toEqual(['story', 'end', 'create', 'zeta', 'other']);
    const story = groups[0]!;
    expect(story.earned).toEqual(['minecraft:story/root', 'minecraft:story/mine_stone']);
    expect(story.total).toBe(VANILLA.story.length);
    expect(story.locked).toHaveLength(VANILLA.story.length - 2);
    expect(story.locked).not.toContain('minecraft:story/root');
    expect(groups[2]).toEqual({ group: 'create', earned: ['create:root'], locked: [], total: null });
  });

  it('keeps a newer version’s extra advancement under its tab, earned', () => {
    const story = groupAchievements(['minecraft:story/brand_new'])[0]!;
    expect(story.earned).toEqual(['minecraft:story/brand_new']);
    expect(story.locked).toHaveLength(VANILLA.story.length);
  });

  it('is empty without IDs', () => {
    expect(groupAchievements([])).toEqual([]);
  });
});
