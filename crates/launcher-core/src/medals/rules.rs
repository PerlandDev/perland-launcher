//! What earns each medal. Pure: the measures in, a tier (0 to 4) per medal out.
use std::collections::{BTreeMap, BTreeSet};

use crate::perland::PlayerStats;

const HOUR: u64 = 3600;

/// Who can earn a medal: anyone using the launcher, or a player on Perland servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Local,
    Perland,
}

/// A ladder has four tiers by thresholds; a single is earned once, at its rim tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ladder,
    Single,
}

/// What a ladder counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Seconds,
    Count,
}

/// One medal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub id: &'static str,
    pub group: Group,
    pub kind: Kind,
    pub unit: Unit,
    /// A ladder's four thresholds, rising; empty for a single.
    pub steps: &'static [u64],
    /// The tier a single shows once earned; 0 for a ladder.
    pub rim: u8,
}

const fn ladder(id: &'static str, group: Group, unit: Unit, steps: &'static [u64]) -> Rule {
    Rule {
        id,
        group,
        kind: Kind::Ladder,
        unit,
        steps,
        rim: 0,
    }
}

const fn single(id: &'static str, group: Group, rim: u8) -> Rule {
    Rule {
        id,
        group,
        kind: Kind::Single,
        unit: Unit::Count,
        steps: &[],
        rim,
    }
}

use Group::{Local, Perland};
use Unit::{Count, Seconds};

/// Every medal in display order: the ones anyone can earn, then Perland's.
pub static CATALOGUE: [Rule; 24] = [
    single("firstGame", Local, 1),
    ladder(
        "time",
        Local,
        Seconds,
        &[HOUR, 10 * HOUR, 100 * HOUR, 500 * HOUR],
    ),
    ladder(
        "instanceTime",
        Local,
        Seconds,
        &[5 * HOUR, 25 * HOUR, 100 * HOUR, 300 * HOUR],
    ),
    ladder("collector", Local, Count, &[2, 5, 15, 30]),
    ladder("modpacks", Local, Count, &[1, 3, 10, 20]),
    ladder("modder", Local, Count, &[10, 50, 200, 500]),
    ladder("versions", Local, Count, &[2, 5, 10, 20]),
    ladder("days", Local, Count, &[7, 30, 100, 365]),
    ladder(
        "marathon",
        Local,
        Seconds,
        &[2 * HOUR, 4 * HOUR, 8 * HOUR, 12 * HOUR],
    ),
    single("theme", Local, 1),
    single("decorator", Local, 1),
    single("tailor", Local, 1),
    single("doctor", Local, 2),
    single("completionist", Local, 4),
    ladder(
        "regular",
        Perland,
        Seconds,
        &[HOUR, 10 * HOUR, 50 * HOUR, 200 * HOUR],
    ),
    ladder("achiever", Perland, Count, &[5, 25, 50, 100]),
    ladder("traveler", Perland, Count, &[1, 2, 3, 5]),
    single("diamonds", Perland, 2),
    single("nether", Perland, 2),
    single("end", Perland, 3),
    single("elytra", Perland, 3),
    single("hero", Perland, 3),
    single("dragon", Perland, 4),
    single("wither", Perland, 4),
];

/// The advancement that earns each Perland single.
const ADVANCEMENTS: [(&str, &str); 7] = [
    ("diamonds", "minecraft:story/mine_diamond"),
    ("nether", "minecraft:story/enter_the_nether"),
    ("end", "minecraft:story/enter_the_end"),
    ("elytra", "minecraft:end/elytra"),
    ("hero", "minecraft:adventure/hero_of_the_village"),
    ("dragon", "minecraft:end/kill_dragon"),
    ("wither", "minecraft:nether/summon_wither"),
];

/// The rule of `id`.
pub fn rule(id: &str) -> Option<&'static Rule> {
    CATALOGUE.iter().find(|r| r.id == id)
}

/// What the launcher measured, for the medals anyone can earn.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Measures {
    /// Seconds played over every instance.
    pub local_seconds: u64,
    /// The most seconds played on one instance.
    pub best_instance_seconds: u64,
    /// Instances the launcher has now.
    pub instances: u64,
    /// Modpacks installed from Explore, ever.
    pub modpacks: u64,
    /// Mods installed from Explore, ever.
    pub mods: u64,
    /// Distinct Minecraft versions played.
    pub versions: u64,
    /// Distinct days with a run.
    pub days: u64,
    /// The longest single run, in seconds.
    pub longest_run_seconds: u64,
    pub first_game: bool,
    pub theme: bool,
    pub decorator: bool,
    pub tailor: bool,
    pub doctor: bool,
}

/// A ladder's reading or a single's 0/1, for the medals anyone can earn.
pub fn local_value(id: &str, m: &Measures) -> u64 {
    let flag = |b: bool| u64::from(b);
    match id {
        "time" => m.local_seconds,
        "instanceTime" => m.best_instance_seconds,
        "collector" => m.instances,
        "modpacks" => m.modpacks,
        "modder" => m.mods,
        "versions" => m.versions,
        "days" => m.days,
        "marathon" => m.longest_run_seconds,
        "firstGame" => flag(m.first_game),
        "theme" => flag(m.theme),
        "decorator" => flag(m.decorator),
        "tailor" => flag(m.tailor),
        "doctor" => flag(m.doctor),
        _ => 0,
    }
}

/// A ladder's reading or a single's 0/1, for Perland's medals.
pub fn perland_value(id: &str, stats: &PlayerStats) -> u64 {
    match id {
        "regular" => stats
            .servers
            .iter()
            .map(|s| s.playtime_seconds)
            .max()
            .unwrap_or(0),
        "achiever" => stats
            .servers
            .iter()
            .flat_map(|s| s.advancement_ids.iter())
            .collect::<BTreeSet<_>>()
            .len() as u64,
        "traveler" => stats
            .servers
            .iter()
            .filter(|s| !s.advancement_ids.is_empty())
            .count() as u64,
        _ => ADVANCEMENTS
            .iter()
            .find(|(medal, _)| *medal == id)
            .map(|(_, adv)| {
                u64::from(
                    stats
                        .servers
                        .iter()
                        .any(|s| s.advancement_ids.iter().any(|a| a == adv)),
                )
            })
            .unwrap_or(0),
    }
}

/// The tier a reading reaches under `rule`.
pub fn tier_of(rule: &Rule, value: u64) -> u8 {
    match rule.kind {
        Kind::Ladder => rule.steps.iter().filter(|&&s| value >= s).count() as u8,
        Kind::Single if value > 0 => rule.rim,
        Kind::Single => 0,
    }
}

/// The tier of every medal anyone can earn. `held` is the highest tier already recorded per
/// medal, which counts for `completionist`: every other local medal earned, ladders at diamond.
pub fn local_tiers(m: &Measures, held: &BTreeMap<String, u8>) -> BTreeMap<&'static str, u8> {
    let mut out = BTreeMap::new();
    for r in CATALOGUE
        .iter()
        .filter(|r| r.group == Local && r.id != "completionist")
    {
        out.insert(r.id, tier_of(r, local_value(r.id, m)));
    }
    let done = out.iter().all(|(id, &tier)| {
        let tier = tier.max(held.get(*id).copied().unwrap_or(0));
        match rule(id).map(|r| r.kind) {
            Some(Kind::Ladder) => tier >= 4,
            _ => tier > 0,
        }
    });
    out.insert("completionist", if done { 4 } else { 0 });
    out
}

/// The tier of every Perland medal.
pub fn perland_tiers(stats: &PlayerStats) -> BTreeMap<&'static str, u8> {
    CATALOGUE
        .iter()
        .filter(|r| r.group == Perland)
        .map(|r| (r.id, tier_of(r, perland_value(r.id, stats))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perland::ServerStats;

    fn server(seconds: u64, ids: &[&str]) -> ServerStats {
        ServerStats {
            name: "s".into(),
            invite: None,
            playtime_seconds: seconds,
            advancements: ids.len() as u32,
            advancement_ids: ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn the_catalogue_has_24_unique_medals_and_14_anyone_can_earn() {
        let ids: BTreeSet<_> = CATALOGUE.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 24);
        assert_eq!(CATALOGUE.iter().filter(|r| r.group == Local).count(), 14);
        for r in &CATALOGUE {
            match r.kind {
                Kind::Ladder => assert!(
                    r.steps.len() == 4 && r.steps.windows(2).all(|w| w[0] < w[1]),
                    "{}",
                    r.id
                ),
                Kind::Single => assert!((1..=4).contains(&r.rim), "{}", r.id),
            }
        }
    }

    #[test]
    fn ladders_reach_a_tier_at_its_threshold() {
        let r = rule("instanceTime").unwrap();
        assert_eq!(tier_of(r, 5 * HOUR - 1), 0);
        assert_eq!(tier_of(r, 5 * HOUR), 1);
        assert_eq!(tier_of(r, 300 * HOUR), 4);
        assert_eq!(tier_of(rule("days").unwrap(), 30), 2);
        assert_eq!(tier_of(rule("marathon").unwrap(), 9000), 1);
    }

    #[test]
    fn singles_show_their_rim() {
        let m = Measures {
            doctor: true,
            tailor: true,
            ..Default::default()
        };
        let t = local_tiers(&m, &BTreeMap::new());
        assert_eq!((t["doctor"], t["tailor"], t["theme"]), (2, 1, 0));
    }

    #[test]
    fn completionist_needs_every_other_local_medal_at_its_best() {
        let mut m = Measures {
            local_seconds: 500 * HOUR,
            best_instance_seconds: 300 * HOUR,
            instances: 30,
            modpacks: 20,
            mods: 500,
            versions: 20,
            days: 365,
            longest_run_seconds: 12 * HOUR,
            first_game: true,
            theme: true,
            decorator: true,
            tailor: true,
            doctor: true,
        };
        assert_eq!(local_tiers(&m, &BTreeMap::new())["completionist"], 4);
        m.instances = 3;
        assert_eq!(local_tiers(&m, &BTreeMap::new())["completionist"], 0);
        let held = BTreeMap::from([("collector".to_string(), 4u8)]);
        assert_eq!(
            local_tiers(&m, &held)["completionist"],
            4,
            "a tier earned before counts"
        );
    }

    #[test]
    fn perland_medals_read_the_stats() {
        let stats = PlayerStats {
            minecraft: None,
            servers: vec![
                server(11 * HOUR, &["minecraft:story/mine_diamond", "a:1", "a:2"]),
                server(HOUR, &["a:1", "a:3", "a:4"]),
                server(0, &[]),
            ],
        };
        let t = perland_tiers(&stats);
        assert_eq!(t["diamonds"], 2);
        assert_eq!(t["nether"], 0);
        assert_eq!(t["regular"], 2);
        assert_eq!(t["achiever"], 1, "five distinct ids");
        assert_eq!(t["traveler"], 2);
    }
}
