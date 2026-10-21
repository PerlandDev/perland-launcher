import type { TFunction } from 'i18next';

/** The game's advancement tabs, in the game's order. */
export const TABS = ['story', 'nether', 'end', 'adventure', 'husbandry'] as const;
export type Tab = (typeof TABS)[number];

/** Minecraft 1.20.1's advancements with a display, by tab, roots first. Each name is `advancements.<tab>.<name>` in i18n.
 *  A newer version's extras still show, as unknown IDs, under their tab. */
export const VANILLA: Record<Tab, readonly string[]> = {
  story: [
    'root', 'mine_stone', 'upgrade_tools', 'smelt_iron', 'obtain_armor', 'lava_bucket', 'iron_tools', 'deflect_arrow',
    'form_obsidian', 'mine_diamond', 'enter_the_nether', 'shiny_gear', 'enchant_item', 'cure_zombie_villager',
    'follow_ender_eye', 'enter_the_end',
  ],
  nether: [
    'root', 'return_to_sender', 'find_bastion', 'obtain_ancient_debris', 'fast_travel', 'find_fortress',
    'obtain_crying_obsidian', 'distract_piglin', 'ride_strider', 'uneasy_alliance', 'loot_bastion', 'use_lodestone',
    'netherite_armor', 'get_wither_skull', 'obtain_blaze_rod', 'charge_respawn_anchor', 'ride_strider_in_overworld_lava',
    'explore_nether', 'summon_wither', 'brew_potion', 'create_beacon', 'all_potions', 'create_full_beacon', 'all_effects',
  ],
  end: ['root', 'kill_dragon', 'dragon_egg', 'enter_end_gateway', 'respawn_dragon', 'dragon_breath', 'find_end_city', 'elytra', 'levitate'],
  adventure: [
    'root', 'voluntary_exile', 'spyglass_at_parrot', 'kill_a_mob', 'read_power_of_chiseled_bookshelf', 'trade',
    'trim_with_any_armor_pattern', 'honey_block_slide', 'ol_betsy', 'lightning_rod_with_villager_no_fire',
    'fall_from_world_height', 'salvage_sherd', 'avoid_vibration', 'sleep_in_bed', 'hero_of_the_village', 'spyglass_at_ghast',
    'throw_trident', 'kill_mob_near_sculk_catalyst', 'shoot_arrow', 'kill_all_mobs', 'totem_of_undying', 'summon_iron_golem',
    'trade_at_world_height', 'trim_with_all_exclusive_armor_patterns', 'two_birds_one_arrow', 'whos_the_pillager_now',
    'arbalistic', 'craft_decorated_pot_using_only_sherds', 'adventuring_time', 'play_jukebox_in_meadows',
    'walk_on_powder_snow_with_leather_boots', 'spyglass_at_dragon', 'very_very_frightening', 'sniper_duel', 'bullseye',
  ],
  husbandry: [
    'root', 'safely_harvest_honey', 'breed_an_animal', 'allay_deliver_item_to_player', 'ride_a_boat_with_a_goat',
    'tame_an_animal', 'make_a_sign_glow', 'fishy_business', 'silk_touch_nest', 'tadpole_in_a_bucket', 'obtain_sniffer_egg',
    'plant_seed', 'wax_on', 'bred_all_animals', 'allay_deliver_cake_to_note_block', 'complete_catalogue', 'tactical_fishing',
    'leash_all_frog_variants', 'feed_snifflet', 'balanced_diet', 'obtain_netherite_hoe', 'wax_off', 'axolotl_in_a_bucket',
    'froglights', 'plant_any_sniffer_seed', 'kill_axolotl_target',
  ],
};

/** An ID split into its namespace and path (`create:andesite_alloy` → `create`, `andesite_alloy`); null when it isn't one. */
function parse(id: string): { namespace: string; path: string } | null {
  const at = id.indexOf(':');
  if (at <= 0 || at === id.length - 1) return null;
  return { namespace: id.slice(0, at), path: id.slice(at + 1) };
}

/** A mod's group: its namespace behind a prefix, so a mod called `end` or `other` never joins a vanilla group. */
const MOD = 'mod:';

/** Where an ID is listed: a vanilla tab, or `mod:` and its mod's namespace (`mod:create`); `other` when it isn't an ID. */
export function groupOf(id: string): Tab | string {
  const parts = parse(id);
  if (!parts) return 'other';
  if (parts.namespace === 'minecraft') {
    const tab = parts.path.split('/')[0] ?? '';
    if ((TABS as readonly string[]).includes(tab) && parts.path.includes('/')) return tab;
    return 'other';
  }
  return MOD + parts.namespace;
}

/** Whether `id` is in the vanilla table, so it has a translated name. */
export function isKnown(id: string): boolean {
  const parts = parse(id);
  if (!parts || parts.namespace !== 'minecraft') return false;
  const [tab = '', name = '', ...rest] = parts.path.split('/');
  return rest.length === 0 && (TABS as readonly string[]).includes(tab) && VANILLA[tab as Tab].includes(name);
}

/** "andesite_alloy" or "kinetics/belt" → "Andesite alloy" / "Belt": the last part of the path, spaced, first letter up. */
function humanize(path: string): string {
  const words = (path.split('/').at(-1) ?? path).replace(/[_.-]+/g, ' ').trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : path;
}

/** The name the launcher shows for an advancement: the translated name for a vanilla one, else a readable form of its ID. */
export function advancementName(id: string, t: TFunction): string {
  if (isKnown(id)) {
    const [tab, name] = id.slice('minecraft:'.length).split('/');
    return t(`advancements.${tab}.${name}`);
  }
  const parts = parse(id);
  return parts ? humanize(parts.path) : id;
}

/** A group's heading: a vanilla tab's name, or a mod's namespace as it is ("create"). */
export function groupName(group: string, t: TFunction): string {
  if (group.startsWith(MOD)) return group.slice(MOD.length);
  if ((TABS as readonly string[]).includes(group) || group === 'other') return t(`achievements.tabs.${group}`);
  return group;
}

export interface AchievementGroup {
  group: string;
  /** Earned, in the table's order for a vanilla tab, else the Engine's (sorted) order. */
  earned: string[];
  /** Vanilla advancements of the tab not earned yet; always empty for mods. */
  locked: string[];
  /** The tab's size in the table, for "5 of 16"; null for mods. */
  total: number | null;
}

/** A server's achievements as the dialog lists them: the five tabs that have anything earned, in the game's order, then each
 *  mod by name, then `other`. A tab with nothing earned is left out, so a modded server isn't a wall of locks. */
export function groupAchievements(ids: readonly string[]): AchievementGroup[] {
  const unique = [...new Set(ids)];
  const byGroup = new Map<string, string[]>();
  for (const id of unique) {
    const group = groupOf(id);
    byGroup.set(group, [...(byGroup.get(group) ?? []), id]);
  }
  const groups: AchievementGroup[] = [];
  for (const tab of TABS) {
    const got = byGroup.get(tab);
    if (!got) continue;
    const ids = VANILLA[tab].map((name) => `minecraft:${tab}/${name}`);
    const known = new Set(ids);
    const earned = [...ids.filter((id) => got.includes(id)), ...got.filter((id) => !known.has(id))];
    groups.push({ group: tab, earned, locked: ids.filter((id) => !got.includes(id)), total: ids.length });
  }
  const mods = [...byGroup.keys()].filter((g) => !(TABS as readonly string[]).includes(g) && g !== 'other').sort();
  for (const mod of [...mods, 'other']) {
    const got = byGroup.get(mod);
    if (got) groups.push({ group: mod, earned: got, locked: [], total: null });
  }
  return groups;
}
