//! The fixes the crash card offers for a recognised cause: only what the launcher can do itself,
//! for this instance (a pack's own mods are the pack's, a Java switch must be one that helps).
use std::collections::HashSet;

use launcher_core::launch::crash::{Crash, CrashCause, ModRef};

use crate::events::CrashFix;

/// The mod files a cause names, which the fixes may want to turn off.
pub(crate) fn files_named(crash: &Crash) -> Vec<String> {
    let mods: Vec<&ModRef> = match &crash.cause {
        CrashCause::ModsClash { module, other } => vec![other, module],
        CrashCause::ModWrongVersion { module }
        | CrashCause::ModFailed { module }
        | CrashCause::ModFileDamaged { module } => vec![module],
        _ => vec![],
    };
    mods.into_iter().filter_map(|m| m.file.clone()).collect()
}

/// The fixes for `crash`, the primary first. `custom_java` is whether the instance runs a Java
/// the player picked; `pack_owned` holds the named files the pack owns; `recommended_java` is
/// the Java major the instance's game version asks for, when known.
pub(crate) fn fixes_for(
    crash: &Crash,
    custom_java: bool,
    pack_owned: &HashSet<String>,
    recommended_java: Option<u32>,
) -> Vec<CrashFix> {
    let disable = |m: &ModRef| {
        let file = m.file.clone()?;
        (!pack_owned.contains(&file)).then(|| CrashFix::DisableMod {
            file,
            name: m.shown().to_string(),
        })
    };
    match &crash.cause {
        CrashCause::OutOfMemory => vec![CrashFix::MoreMemory],
        CrashCause::GraphicsDriver { vendor } => vendor
            .map(|vendor| CrashFix::UpdateDrivers { vendor })
            .into_iter()
            .collect(),
        CrashCause::JavaTooOld { required, .. } => {
            if custom_java && recommended_java.is_some_and(|r| r >= *required) {
                vec![CrashFix::RecommendedJava, CrashFix::JavaSettings]
            } else {
                vec![CrashFix::JavaSettings]
            }
        }
        CrashCause::JavaTooNew if custom_java => vec![CrashFix::RecommendedJava],
        CrashCause::JavaTooNew => vec![CrashFix::JavaSettings],
        CrashCause::ModMissingDependency { missing, .. } => vec![CrashFix::FindMod {
            project: modrinth_project(missing),
        }],
        // Fabric's own advice for a clash is to remove the mod that is broken with.
        CrashCause::ModsClash { module, other } => {
            [other, module].into_iter().filter_map(disable).collect()
        }
        CrashCause::ModWrongVersion { module }
        | CrashCause::ModFailed { module }
        | CrashCause::ModFileDamaged { module } => disable(module).into_iter().collect(),
        CrashCause::GameFilesDamaged => vec![CrashFix::Repair],
    }
}

/// The Modrinth project a mod id most likely is. Ids mostly are slugs; Fabric API's modules and
/// its old id are the common exception.
pub(crate) fn modrinth_project(id: &str) -> String {
    let fabric_api_module = id.starts_with("fabric-")
        && (id == "fabric-api-base"
            || id.ends_with("-v0")
            || id.ends_with("-v1")
            || id.ends_with("-v2")
            || id.ends_with("-v3"));
    match id {
        "fabric" => "fabric-api".into(),
        "quilted_fabric_api" | "qsl" => "qsl".into(),
        _ if fabric_api_module => "fabric-api".into(),
        _ => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use launcher_core::launch::crash::Vendor;

    fn module(id: &str, file: Option<&str>) -> ModRef {
        ModRef {
            id: id.into(),
            name: Some(id.to_uppercase()),
            file: file.map(Into::into),
        }
    }

    fn crash(cause: CrashCause) -> Crash {
        Crash {
            cause,
            detail: String::new(),
        }
    }

    fn fixes(
        cause: CrashCause,
        custom_java: bool,
        owned: &[&str],
        java: Option<u32>,
    ) -> Vec<CrashFix> {
        let owned = owned.iter().map(|s| s.to_string()).collect();
        fixes_for(&crash(cause), custom_java, &owned, java)
    }

    #[test]
    fn out_of_memory_offers_more_memory() {
        assert_eq!(
            fixes(CrashCause::OutOfMemory, false, &[], None),
            [CrashFix::MoreMemory]
        );
    }

    #[test]
    fn a_driver_link_needs_a_vendor() {
        let amd = CrashCause::GraphicsDriver {
            vendor: Some(Vendor::Amd),
        };
        assert_eq!(
            fixes(amd, false, &[], None),
            [CrashFix::UpdateDrivers {
                vendor: Vendor::Amd
            }]
        );
        assert!(fixes(
            CrashCause::GraphicsDriver { vendor: None },
            false,
            &[],
            None
        )
        .is_empty());
    }

    #[test]
    fn java_too_old_with_a_custom_java_offers_the_recommended_one() {
        let cause = || CrashCause::JavaTooOld {
            required: 17,
            running: Some(8),
        };
        assert_eq!(
            fixes(cause(), true, &[], Some(17)),
            [CrashFix::RecommendedJava, CrashFix::JavaSettings]
        );
        // The recommended Java isn't new enough: only the settings help.
        assert_eq!(fixes(cause(), true, &[], Some(8)), [CrashFix::JavaSettings]);
        assert_eq!(fixes(cause(), true, &[], None), [CrashFix::JavaSettings]);
    }

    #[test]
    fn java_too_old_on_automatic_opens_settings() {
        let cause = CrashCause::JavaTooOld {
            required: 21,
            running: Some(17),
        };
        assert_eq!(fixes(cause, false, &[], Some(17)), [CrashFix::JavaSettings]);
    }

    #[test]
    fn java_too_new() {
        assert_eq!(
            fixes(CrashCause::JavaTooNew, true, &[], Some(8)),
            [CrashFix::RecommendedJava]
        );
        assert_eq!(
            fixes(CrashCause::JavaTooNew, false, &[], Some(8)),
            [CrashFix::JavaSettings]
        );
    }

    #[test]
    fn a_missing_dependency_is_looked_up_on_modrinth() {
        let cause = CrashCause::ModMissingDependency {
            module: module("lithium", Some("lithium.jar")),
            missing: "fabric-resource-loader-v0".into(),
        };
        assert_eq!(
            fixes(cause, false, &[], None),
            [CrashFix::FindMod {
                project: "fabric-api".into()
            }]
        );
    }

    #[test]
    fn fabric_module_ids_map_to_fabric_api() {
        for id in [
            "fabric",
            "fabric-api-base",
            "fabric-networking-api-v1",
            "fabric-api",
        ] {
            assert_eq!(modrinth_project(id), "fabric-api", "{id}");
        }
        assert_eq!(modrinth_project("quilted_fabric_api"), "qsl");
        assert_eq!(
            modrinth_project("fabric-language-kotlin"),
            "fabric-language-kotlin"
        );
        assert_eq!(modrinth_project("flywheel"), "flywheel");
    }

    #[test]
    fn a_clash_offers_the_second_mod_first() {
        let cause = CrashCause::ModsClash {
            module: module("sodium", Some("sodium.jar")),
            other: module("optifabric", Some("optifabric.jar")),
        };
        assert_eq!(
            fixes(cause, false, &[], None),
            [
                CrashFix::DisableMod {
                    file: "optifabric.jar".into(),
                    name: "OPTIFABRIC".into()
                },
                CrashFix::DisableMod {
                    file: "sodium.jar".into(),
                    name: "SODIUM".into()
                },
            ]
        );
    }

    #[test]
    fn pack_mod_gets_no_disable() {
        let cause = CrashCause::ModFailed {
            module: module("create", Some("create.jar")),
        };
        assert!(fixes(cause, false, &["create.jar"], None).is_empty());
    }

    #[test]
    fn a_mod_not_on_disk_gets_no_disable() {
        let cause = CrashCause::ModWrongVersion {
            module: module("jei", None),
        };
        assert!(fixes(cause, false, &[], None).is_empty());
    }

    #[test]
    fn damaged_files() {
        let damaged = |file: Option<&str>| CrashCause::ModFileDamaged {
            module: ModRef {
                id: "sodium.jar".into(),
                name: Some("Sodium".into()),
                file: file.map(Into::into),
            },
        };
        assert_eq!(
            fixes(damaged(Some("sodium.jar")), false, &[], None),
            [CrashFix::DisableMod {
                file: "sodium.jar".into(),
                name: "Sodium".into()
            }]
        );
        // Not an enabled jar on disk: nothing to turn off.
        assert!(fixes(damaged(None), false, &[], None).is_empty());
        assert_eq!(
            fixes(CrashCause::GameFilesDamaged, false, &[], None),
            [CrashFix::Repair]
        );
    }

    #[test]
    fn files_named_lists_the_mods_on_disk() {
        let clash = crash(CrashCause::ModsClash {
            module: module("sodium", Some("sodium.jar")),
            other: module("optifabric", None),
        });
        assert_eq!(files_named(&clash), ["sodium.jar"]);
        assert!(files_named(&crash(CrashCause::OutOfMemory)).is_empty());
    }
}
