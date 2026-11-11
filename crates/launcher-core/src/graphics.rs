//! Which graphics card the game starts on (design: `2026-11-11-graphics-card-choice-design.md`).
//!
//! The system never asks "integrated or dedicated": it knows a power-saving card and a
//! high-performance one. `GraphicsCard::Integrated` asks for the first and `Dedicated` for the
//! second, whatever the two cards are.
//!
//! - Windows: the per-app preference Windows keeps for the Java executable, the one its own
//!   Graphics settings write.
//! - Linux: environment variables on the game's process, for Mesa and for NVIDIA's driver.
//! - macOS: the system decides and the launcher has no say.
use std::path::Path;

use serde::Serialize;

use crate::{
    platform::{Os, Platform},
    settings::GraphicsCard,
};

/// A graphics card found in this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// What to call it: the driver's name on Windows, the maker on Linux.
    pub name: String,
    /// Built into the processor. Only said when it is certain.
    pub integrated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GraphicsStatus {
    /// The player can choose.
    Choose,
    /// One card: there is nothing to choose.
    SingleCard,
    /// The launcher can't choose on this system.
    Unsupported,
}

/// What Settings needs to draw the graphics card choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsSupport {
    pub status: GraphicsStatus,
    /// The cards found, by name. Empty when they couldn't be read.
    pub cards: Vec<String>,
    /// One of the cards is built into the processor, so "integrated" and "dedicated" are the
    /// right words. Otherwise the choice is between less power and more performance.
    pub integrated: bool,
}

impl GraphicsSupport {
    /// Nothing is known about the cards and the choice stays open: tests and the dev harness.
    pub fn unknown() -> Self {
        support(Os::Linux, None)
    }
}

/// `cards` is `None` when they couldn't be read. That never locks the choice: a choice that
/// does nothing is better than a lock on a computer that has two cards.
pub fn support(os: Os, cards: Option<Vec<Card>>) -> GraphicsSupport {
    let cards = cards.unwrap_or_default();
    let status = match (os, cards.len()) {
        (Os::MacOs, _) => GraphicsStatus::Unsupported,
        (_, 1) => GraphicsStatus::SingleCard,
        _ => GraphicsStatus::Choose,
    };
    GraphicsSupport {
        status,
        integrated: cards.iter().any(|c| c.integrated),
        cards: cards.into_iter().map(|c| c.name).collect(),
    }
}

/// This computer's cards and whether the player can choose between them.
pub fn detect() -> GraphicsSupport {
    support(Platform::current().os, detect_cards())
}

#[cfg(target_os = "linux")]
fn detect_cards() -> Option<Vec<Card>> {
    linux_cards(Path::new("/sys/class/drm"))
}

#[cfg(windows)]
fn detect_cards() -> Option<Vec<Card>> {
    windows::cards()
}

#[cfg(not(any(target_os = "linux", windows)))]
fn detect_cards() -> Option<Vec<Card>> {
    None
}

const INTEL: u16 = 0x8086;
const NVIDIA: u16 = 0x10de;
const AMD: u16 = 0x1002;

/// Built into the processor? Only a certain yes: a wrong one would put the wrong words on the
/// choice, and a no only makes the words more general.
pub fn is_integrated(vendor: Option<u16>, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if vendor == Some(INTEL) || name.contains("intel") {
        // Intel's Arc cards are the dedicated ones; "Arc Graphics" in a laptop processor is not
        // told apart, and reads as dedicated.
        return !name.contains("arc");
    }
    // AMD names its processor graphics "Radeon Graphics" or "Radeon Vega", without a model.
    name.contains("radeon(tm) graphics")
        || name.ends_with("radeon graphics")
        || name.contains("vega")
}

/// The cards the kernel lists under `/sys/class/drm` (`dir`): one `card<N>` folder each, with
/// the maker in `device/vendor`. `None` when the folder can't be read.
#[cfg(any(target_os = "linux", test))]
fn linux_cards(dir: &Path) -> Option<Vec<Card>> {
    let mut found: Vec<(String, Card)> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // `card0-eDP-1` is a connector of `card0`.
        let Some(number) = name.strip_prefix("card") else {
            continue;
        };
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        // A card without a maker is a software one (`simpledrm`, a virtual display).
        let Ok(raw) = std::fs::read_to_string(entry.path().join("device/vendor")) else {
            continue;
        };
        let Ok(vendor) = u16::from_str_radix(raw.trim().trim_start_matches("0x"), 16) else {
            continue;
        };
        let maker = match vendor {
            INTEL => "Intel".to_string(),
            NVIDIA => "NVIDIA".to_string(),
            AMD => "AMD".to_string(),
            other => format!("{other:#06x}"),
        };
        let card = Card {
            integrated: is_integrated(Some(vendor), &maker),
            name: maker,
        };
        found.push((name, card));
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Some(found.into_iter().map(|(_, card)| card).collect())
}

/// The variables that put the game on the chosen card on Linux. Mesa reads `DRI_PRIME`;
/// NVIDIA's own driver reads the rest, and each ignores the other's.
pub fn launch_env(os: Os, choice: GraphicsCard) -> Vec<(String, String)> {
    let vars: &[(&str, &str)] = match (os, choice) {
        (Os::Linux, GraphicsCard::Dedicated) => &[
            ("DRI_PRIME", "1"),
            ("__NV_PRIME_RENDER_OFFLOAD", "1"),
            ("__GLX_VENDOR_LIBRARY_NAME", "nvidia"),
            ("__VK_LAYER_NV_optimus", "NVIDIA_only"),
        ],
        (Os::Linux, GraphicsCard::Integrated) => &[("DRI_PRIME", "0")],
        _ => &[],
    };
    vars.iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// What Windows stores for an executable that should run on the chosen card.
pub fn windows_preference(choice: GraphicsCard) -> Option<&'static str> {
    match choice {
        GraphicsCard::Dedicated => Some("GpuPreference=2;"),
        GraphicsCard::Integrated => Some("GpuPreference=1;"),
        GraphicsCard::Automatic => None,
    }
}

/// Windows' per-app graphics preferences, keyed by the executable's full path.
pub trait Preferences {
    fn get(&self, exe: &str) -> Option<String>;
    fn set(&mut self, exe: &str, value: &str) -> std::io::Result<()>;
    fn remove(&mut self, exe: &str) -> std::io::Result<()>;
}

/// Make Windows' preference for `java` match `choice`. `owned` lists the executables whose
/// preference the launcher wrote: going back to Automatic removes those and nothing else, so a
/// preference the player set in Windows themselves is never taken away.
pub fn apply(
    prefs: &mut dyn Preferences,
    owned: &mut Vec<String>,
    java: &str,
    choice: GraphicsCard,
) -> std::io::Result<()> {
    let Some(value) = windows_preference(choice) else {
        for exe in std::mem::take(owned) {
            // Changed since (in Windows' settings): it is the player's now.
            let ours = prefs.get(&exe).is_some_and(|v| {
                [GraphicsCard::Dedicated, GraphicsCard::Integrated]
                    .iter()
                    .any(|c| windows_preference(*c) == Some(v.as_str()))
            });
            if ours {
                prefs.remove(&exe)?;
            }
        }
        return Ok(());
    };
    if prefs.get(java).as_deref() == Some(value) {
        // Already so. Not added to `owned`: if the player set it, it stays theirs.
        return Ok(());
    }
    prefs.set(java, value)?;
    if !owned.iter().any(|exe| exe == java) {
        owned.push(java.to_string());
    }
    Ok(())
}

/// Before a game starts: tell Windows which card `java` runs on. `owned_file` remembers what the
/// launcher wrote. Does nothing on other systems, where `launch_env` does the work.
pub async fn apply_at_launch(
    owned_file: &Path,
    java: &Path,
    choice: GraphicsCard,
) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let mut owned: Vec<String> = match tokio::fs::read(owned_file).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        if owned.is_empty() && choice == GraphicsCard::Automatic {
            return Ok(());
        }
        let before = owned.clone();
        let result = apply(
            &mut windows::Registry,
            &mut owned,
            &java.to_string_lossy(),
            choice,
        );
        if owned != before {
            let bytes = serde_json::to_vec_pretty(&owned).map_err(std::io::Error::other)?;
            crate::fsutil::write_atomic(owned_file, &bytes)
                .await
                .map_err(|e| std::io::Error::other(e.to_string()))?;
        }
        result
    }
    #[cfg(not(windows))]
    {
        let _ = (owned_file, java, choice);
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

    use super::{is_integrated, video_key, Card, Preferences};

    const PREFERENCES: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";

    fn io(e: windows_registry::Error) -> std::io::Error {
        std::io::Error::other(e.to_string())
    }

    pub struct Registry;

    impl Preferences for Registry {
        fn get(&self, exe: &str) -> Option<String> {
            CURRENT_USER.open(PREFERENCES).ok()?.get_string(exe).ok()
        }
        fn set(&mut self, exe: &str, value: &str) -> std::io::Result<()> {
            CURRENT_USER
                .create(PREFERENCES)
                .and_then(|key| key.set_string(exe, value))
                .map_err(io)
        }
        fn remove(&mut self, exe: &str) -> std::io::Result<()> {
            CURRENT_USER
                .create(PREFERENCES)
                .and_then(|key| key.remove_value(exe))
                .map_err(io)
        }
    }

    /// The cards in use since this start of Windows: `HARDWARE\DEVICEMAP\VIDEO` is rebuilt at
    /// every start, so a card that was taken out is not listed. Each `\Device\Video<N>` names a
    /// key of the adapter it belongs to; several of them share one adapter.
    pub fn cards() -> Option<Vec<Card>> {
        let map = LOCAL_MACHINE.open(r"HARDWARE\DEVICEMAP\VIDEO").ok()?;
        let mut seen: Vec<String> = Vec::new();
        let mut cards = Vec::new();
        for n in 0..32 {
            let Ok(target) = map.get_string(format!(r"\Device\Video{n}")) else {
                continue;
            };
            let Some((adapter, path)) = video_key(&target) else {
                continue;
            };
            if seen.contains(&adapter) {
                continue;
            }
            let Ok(key) = LOCAL_MACHINE.open(&path) else {
                continue;
            };
            // A card in a slot or in the processor is a PCI device. Remote-desktop and other
            // software displays are not.
            let Ok(hardware) = key.get_string("MatchingDeviceId") else {
                continue;
            };
            let Some(vendor) = super::pci_vendor(&hardware) else {
                continue;
            };
            let name = key
                .get_string("DriverDesc")
                .unwrap_or_else(|_| hardware.clone());
            seen.push(adapter);
            cards.push(Card {
                integrated: is_integrated(Some(vendor), &name),
                name,
            });
        }
        Some(cards)
    }
}

/// From `\Registry\Machine\System\CurrentControlSet\Control\Video\{GUID}\0000`: the adapter
/// (`{GUID}`, lower case) and the key's path under `HKEY_LOCAL_MACHINE`.
#[cfg(any(windows, test))]
fn video_key(target: &str) -> Option<(String, String)> {
    const PREFIX: &str = r"\registry\machine\";
    let lower = target.to_ascii_lowercase();
    if !lower.starts_with(PREFIX) {
        return None;
    }
    let path = &target[PREFIX.len()..];
    let mut parts = lower[PREFIX.len()..].split('\\');
    let adapter = parts.find(|part| part.starts_with('{') && part.ends_with('}'))?;
    Some((adapter.to_string(), path.to_string()))
}

/// The maker in a hardware id like `pci\ven_10de&dev_2504&subsys_…`. `None` when it isn't PCI.
#[cfg(any(windows, test))]
fn pci_vendor(hardware_id: &str) -> Option<u16> {
    let lower = hardware_id.to_ascii_lowercase();
    let rest = lower.strip_prefix(r"pci\ven_")?;
    u16::from_str_radix(rest.get(..4)?, 16).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn card(name: &str, integrated: bool) -> Card {
        Card {
            name: name.into(),
            integrated,
        }
    }

    #[test]
    fn two_cards_can_be_chosen_and_one_cannot() {
        let laptop = support(
            Os::Windows,
            Some(vec![
                card("Intel(R) UHD Graphics", true),
                card("NVIDIA GeForce RTX 4060", false),
            ]),
        );
        assert_eq!(laptop.status, GraphicsStatus::Choose);
        assert_eq!(
            laptop.cards,
            ["Intel(R) UHD Graphics", "NVIDIA GeForce RTX 4060"]
        );
        assert!(laptop.integrated);

        let one = support(Os::Linux, Some(vec![card("NVIDIA", false)]));
        assert_eq!(one.status, GraphicsStatus::SingleCard);
        assert_eq!(one.cards, ["NVIDIA"]);
    }

    #[test]
    fn two_dedicated_cards_are_a_choice_without_an_integrated_one() {
        let tower = support(
            Os::Windows,
            Some(vec![
                card("NVIDIA GeForce RTX 4090", false),
                card("AMD Radeon RX 580", false),
            ]),
        );
        assert_eq!(tower.status, GraphicsStatus::Choose);
        assert!(!tower.integrated);
    }

    #[test]
    fn three_cards_are_still_a_choice() {
        let cards = vec![
            card("Intel", true),
            card("NVIDIA", false),
            card("AMD", false),
        ];
        assert_eq!(
            support(Os::Linux, Some(cards)).status,
            GraphicsStatus::Choose
        );
    }

    #[test]
    fn unread_cards_never_lock_the_choice() {
        for cards in [None, Some(vec![])] {
            let unknown = support(Os::Windows, cards);
            assert_eq!(unknown.status, GraphicsStatus::Choose);
            assert!(unknown.cards.is_empty());
            assert!(!unknown.integrated);
        }
        assert_eq!(GraphicsSupport::unknown().status, GraphicsStatus::Choose);
    }

    #[test]
    fn macos_is_unsupported_whatever_it_has() {
        for cards in [None, Some(vec![card("Apple M2", true)])] {
            assert_eq!(
                support(Os::MacOs, cards).status,
                GraphicsStatus::Unsupported
            );
        }
    }

    #[test]
    fn support_serializes_in_camel_case() {
        let json =
            serde_json::to_value(support(Os::Linux, Some(vec![card("Intel", true)]))).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"status": "singleCard", "cards": ["Intel"], "integrated": true})
        );
    }

    #[test]
    fn integrated_is_only_said_when_certain() {
        assert!(is_integrated(Some(INTEL), "Intel(R) UHD Graphics 770"));
        assert!(is_integrated(None, "Intel(R) Iris(R) Xe Graphics"));
        assert!(is_integrated(Some(AMD), "AMD Radeon(TM) Graphics"));
        assert!(is_integrated(Some(AMD), "AMD Radeon Graphics"));
        assert!(is_integrated(Some(AMD), "AMD Radeon(TM) Vega 8 Graphics"));
        assert!(!is_integrated(
            Some(INTEL),
            "Intel(R) Arc(TM) A770 Graphics"
        ));
        assert!(!is_integrated(Some(AMD), "AMD Radeon RX 6600 XT"));
        assert!(!is_integrated(Some(AMD), "AMD"));
        assert!(!is_integrated(
            Some(NVIDIA),
            "NVIDIA GeForce RTX 4060 Laptop GPU"
        ));
    }

    fn drm(cards: &[(&str, Option<&str>)]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (name, vendor) in cards {
            let device = tmp.path().join(name).join("device");
            std::fs::create_dir_all(&device).unwrap();
            if let Some(vendor) = vendor {
                std::fs::write(device.join("vendor"), format!("{vendor}\n")).unwrap();
            }
        }
        tmp
    }

    #[test]
    fn linux_counts_cards_not_connectors_or_software_displays() {
        let tmp = drm(&[
            ("card1", Some("0x10de")),
            ("card0", Some("0x8086")),
            ("card0-eDP-1", Some("0x8086")),
            ("card1-HDMI-A-1", None),
            ("card2", None),
            ("renderD128", Some("0x8086")),
            ("version", None),
        ]);
        assert_eq!(
            linux_cards(tmp.path()),
            Some(vec![card("Intel", true), card("NVIDIA", false)])
        );
    }

    #[test]
    fn linux_names_amd_and_unknown_makers() {
        let tmp = drm(&[("card0", Some("0x1002")), ("card1", Some("0x1af4"))]);
        assert_eq!(
            linux_cards(tmp.path()),
            Some(vec![card("AMD", false), card("0x1af4", false)])
        );
    }

    #[test]
    fn linux_without_the_folder_is_unknown() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(linux_cards(&tmp.path().join("missing")), None);
        assert_eq!(linux_cards(tmp.path()), Some(vec![]));
    }

    fn env(os: Os, choice: GraphicsCard) -> BTreeMap<String, String> {
        launch_env(os, choice).into_iter().collect()
    }

    #[test]
    fn linux_env_asks_for_the_chosen_card() {
        let dedicated = env(Os::Linux, GraphicsCard::Dedicated);
        assert_eq!(dedicated["DRI_PRIME"], "1");
        assert_eq!(dedicated["__NV_PRIME_RENDER_OFFLOAD"], "1");
        assert_eq!(dedicated["__GLX_VENDOR_LIBRARY_NAME"], "nvidia");
        assert_eq!(dedicated["__VK_LAYER_NV_optimus"], "NVIDIA_only");
        assert_eq!(
            env(Os::Linux, GraphicsCard::Integrated),
            BTreeMap::from([("DRI_PRIME".to_string(), "0".to_string())])
        );
    }

    #[test]
    fn automatic_and_other_systems_set_no_env() {
        assert!(env(Os::Linux, GraphicsCard::Automatic).is_empty());
        for os in [Os::Windows, Os::MacOs] {
            for choice in [GraphicsCard::Dedicated, GraphicsCard::Integrated] {
                assert!(env(os, choice).is_empty());
            }
        }
    }

    #[derive(Default)]
    struct Fake {
        values: BTreeMap<String, String>,
        fail: bool,
    }

    impl Preferences for Fake {
        fn get(&self, exe: &str) -> Option<String> {
            self.values.get(exe).cloned()
        }
        fn set(&mut self, exe: &str, value: &str) -> std::io::Result<()> {
            if self.fail {
                return Err(std::io::Error::other("denied"));
            }
            self.values.insert(exe.into(), value.into());
            Ok(())
        }
        fn remove(&mut self, exe: &str) -> std::io::Result<()> {
            self.values.remove(exe);
            Ok(())
        }
    }

    const JAVA: &str = r"C:\launcher\java\bin\javaw.exe";
    const OTHER: &str = r"C:\Program Files\Java\bin\javaw.exe";

    #[test]
    fn a_choice_is_written_for_the_java_and_remembered() {
        let (mut prefs, mut owned) = (Fake::default(), vec![]);
        apply(&mut prefs, &mut owned, JAVA, GraphicsCard::Dedicated).unwrap();
        assert_eq!(prefs.values[JAVA], "GpuPreference=2;");
        assert_eq!(owned, [JAVA]);

        apply(&mut prefs, &mut owned, JAVA, GraphicsCard::Integrated).unwrap();
        assert_eq!(prefs.values[JAVA], "GpuPreference=1;");
        assert_eq!(owned, [JAVA]);

        apply(&mut prefs, &mut owned, OTHER, GraphicsCard::Integrated).unwrap();
        assert_eq!(owned, [JAVA, OTHER]);
    }

    #[test]
    fn automatic_removes_what_the_launcher_wrote() {
        let (mut prefs, mut owned) = (Fake::default(), vec![]);
        apply(&mut prefs, &mut owned, JAVA, GraphicsCard::Dedicated).unwrap();
        apply(&mut prefs, &mut owned, OTHER, GraphicsCard::Dedicated).unwrap();
        apply(&mut prefs, &mut owned, JAVA, GraphicsCard::Automatic).unwrap();
        assert!(prefs.values.is_empty());
        assert!(owned.is_empty());
    }

    #[test]
    fn automatic_leaves_the_players_own_preference_alone() {
        let mut prefs = Fake::default();
        // Set by the player in Windows' Graphics settings, before the launcher had a choice.
        prefs.values.insert(JAVA.into(), "GpuPreference=2;".into());
        let mut owned = vec![];
        apply(&mut prefs, &mut owned, JAVA, GraphicsCard::Automatic).unwrap();
        assert_eq!(prefs.values[JAVA], "GpuPreference=2;");

        // The same value chosen in the launcher changes nothing, and stays the player's.
        apply(&mut prefs, &mut owned, JAVA, GraphicsCard::Dedicated).unwrap();
        assert!(owned.is_empty());
        apply(&mut prefs, &mut owned, JAVA, GraphicsCard::Automatic).unwrap();
        assert_eq!(prefs.values[JAVA], "GpuPreference=2;");
    }

    #[test]
    fn automatic_leaves_a_preference_changed_in_windows_since() {
        let (mut prefs, mut owned) = (Fake::default(), vec![]);
        apply(&mut prefs, &mut owned, JAVA, GraphicsCard::Dedicated).unwrap();
        // Windows 11 writes more than the launcher does when the player picks a card by name.
        let theirs = "SpecificAdapter=4318&9479&0;GpuPreference=1073741824;";
        prefs.values.insert(JAVA.into(), theirs.into());
        apply(&mut prefs, &mut owned, JAVA, GraphicsCard::Automatic).unwrap();
        assert_eq!(prefs.values[JAVA], theirs);
        assert!(owned.is_empty());
    }

    #[test]
    fn a_refused_write_is_an_error_and_remembers_nothing() {
        let mut prefs = Fake {
            fail: true,
            ..Fake::default()
        };
        let mut owned = vec![];
        assert!(apply(&mut prefs, &mut owned, JAVA, GraphicsCard::Dedicated).is_err());
        assert!(owned.is_empty());
    }

    #[tokio::test]
    async fn applying_at_launch_off_windows_does_nothing() {
        if cfg!(windows) {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("graphics.json");
        apply_at_launch(&file, Path::new("/usr/bin/java"), GraphicsCard::Dedicated)
            .await
            .unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn video_keys_name_their_adapter() {
        let target = r"\Registry\Machine\System\CurrentControlSet\Control\Video\{AB12CD34-0000-1111-2222-333344445555}\0001";
        assert_eq!(
            video_key(target),
            Some((
                "{ab12cd34-0000-1111-2222-333344445555}".to_string(),
                r"System\CurrentControlSet\Control\Video\{AB12CD34-0000-1111-2222-333344445555}\0001"
                    .to_string()
            ))
        );
        assert_eq!(video_key(r"\Registry\Machine\System\Nothing"), None);
        assert_eq!(video_key(r"\Device\Video0"), None);
    }

    #[test]
    fn pci_hardware_ids_give_the_maker() {
        assert_eq!(
            pci_vendor(r"PCI\VEN_10DE&DEV_2504&SUBSYS_00000000"),
            Some(NVIDIA)
        );
        assert_eq!(pci_vendor(r"pci\ven_8086&dev_a780"), Some(INTEL));
        assert_eq!(pci_vendor(r"ROOT\BasicDisplay"), None);
        assert_eq!(pci_vendor(r"pci\ven_"), None);
    }
}
