use std::collections::HashSet;

use super::{
    maven::MavenCoord,
    mojang::{ArgValue, Arguments, VersionJson, LEGACY_JVM_ARGS},
};

/// Apply a loader profile (`child`, with `inheritsFrom`) on top of the vanilla version (`parent`).
pub fn merge(parent: VersionJson, child: VersionJson) -> VersionJson {
    let child_ids: HashSet<String> = child
        .libraries
        .iter()
        .filter_map(|l| MavenCoord::parse(&l.name))
        .map(|c| c.identity())
        .collect();
    let mut libraries = child.libraries;
    libraries.extend(parent.libraries.into_iter().filter(|l| {
        MavenCoord::parse(&l.name)
            .map(|c| !child_ids.contains(&c.identity()))
            .unwrap_or(true)
    }));
    let legacy = child.minecraft_arguments.or(parent.minecraft_arguments);
    let (arguments, minecraft_arguments) = match (parent.arguments, child.arguments, legacy) {
        (Some(p), Some(c), legacy) => (
            Some(Arguments {
                game: [p.game, c.game].concat(),
                jvm: [p.jvm, c.jvm].concat(),
            }),
            legacy,
        ),
        // A legacy parent (one `minecraftArguments` string) under a modern child: rewrite the
        // parent in the modern shape, or the child's arguments would replace all of its own.
        (None, Some(c), Some(legacy)) => (Some(modernize(&legacy, c)), None),
        (p, c, legacy) => (c.or(p), legacy),
    };
    VersionJson {
        id: child.id,
        inherits_from: None,
        main_class: child.main_class,
        arguments,
        minecraft_arguments,
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets),
        downloads: child.downloads.or(parent.downloads),
        java_version: child.java_version.or(parent.java_version),
        libraries,
        // Forge ships `"logging": {}`, which must not erase vanilla's log config.
        logging: child
            .logging
            .filter(|l| l.client.is_some())
            .or(parent.logging),
        kind: child.kind.or(parent.kind),
    }
}

/// A legacy argument string plus a modern child, as one modern `arguments` block. The legacy JVM
/// defaults are only spelled out when the child adds JVM arguments: with none, the launcher's
/// own legacy defaults apply anyway.
fn modernize(legacy: &str, child: Arguments) -> Arguments {
    let plain = |s: &str| ArgValue::Plain(s.to_string());
    Arguments {
        game: legacy
            .split_whitespace()
            .map(plain)
            .chain(child.game)
            .collect(),
        jvm: if child.jvm.is_empty() {
            Vec::new()
        } else {
            LEGACY_JVM_ARGS
                .iter()
                .copied()
                .map(plain)
                .chain(child.jvm)
                .collect()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::version;

    #[test]
    fn fabric_profile_merges_onto_vanilla() {
        let merged = merge(
            version("1.20.1.json"),
            version("fabric-1.20.1-0.16.14.json"),
        );
        assert_eq!(merged.id, "fabric-loader-0.16.14-1.20.1");
        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert!(merged.inherits_from.is_none());
        assert_eq!(merged.libraries.len(), 8 + 88);
        assert_eq!(merged.libraries[0].name, "org.ow2.asm:asm:9.8");
        assert_eq!(merged.asset_index.as_ref().unwrap().id, "5");
        assert!(
            merged.downloads.is_some(),
            "client download comes from vanilla"
        );
        let jvm = &merged.arguments.as_ref().unwrap().jvm;
        assert_eq!(
            jvm.last().unwrap(),
            &ArgValue::Plain("-DFabricMcEmu= net.minecraft.client.main.Main ".into())
        );
        assert_eq!(merged.java_version.as_ref().unwrap().major_version, 17);
    }

    #[test]
    fn child_library_replaces_same_identity() {
        let parent: VersionJson = serde_json::from_str(
            r#"{"id":"p","mainClass":"P","libraries":[{"name":"a:b:1"},{"name":"c:d:1"}]}"#,
        )
        .unwrap();
        let child: VersionJson =
            serde_json::from_str(r#"{"id":"c","mainClass":"C","libraries":[{"name":"a:b:2"}]}"#)
                .unwrap();
        let names: Vec<String> = merge(parent, child)
            .libraries
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, vec!["a:b:2", "c:d:1"]);
    }

    #[test]
    fn forge_profile_merges_onto_vanilla() {
        let merged = merge(
            version("1.20.1.json"),
            version("forge-1.20.1-47.4.22-version.json"),
        );
        assert_eq!(merged.id, "1.20.1-forge-47.4.22");
        assert_eq!(
            merged.main_class,
            "cpw.mods.bootstraplauncher.BootstrapLauncher"
        );
        assert_eq!(merged.libraries.len(), 29 + 88);
        assert_eq!(merged.java_version.as_ref().unwrap().major_version, 17);
        let game = &merged.arguments.as_ref().unwrap().game;
        assert!(game.contains(&ArgValue::Plain("forgeclient".into())));
        assert!(
            game.contains(&ArgValue::Plain("--username".into())),
            "vanilla's game arguments stay"
        );
    }

    #[test]
    fn empty_child_logging_keeps_vanilla_logging() {
        // Forge 47.4.22's version.json says `"logging": {}`.
        let merged = merge(
            version("1.20.1.json"),
            version("forge-1.20.1-47.4.22-version.json"),
        );
        let client = merged.logging.unwrap().client.unwrap();
        assert_eq!(client.file.id, "client-1.12.xml");
    }

    #[test]
    fn legacy_parent_with_modern_child_keeps_every_game_argument() {
        let parent: VersionJson = serde_json::from_str(
            r#"{"id":"1.12.2","mainClass":"net.minecraft.client.main.Main",
                "minecraftArguments":"--username ${auth_player_name} --version ${version_name}"}"#,
        )
        .unwrap();
        let child: VersionJson = serde_json::from_str(
            r#"{"id":"x","mainClass":"Y","arguments":{"game":["--tweakClass","T"],"jvm":["-Dx=1"]}}"#,
        )
        .unwrap();
        let merged = merge(parent, child);
        assert!(merged.minecraft_arguments.is_none());
        let a = merged.arguments.unwrap();
        let plain = |v: &[ArgValue]| -> Vec<String> {
            v.iter()
                .map(|a| match a {
                    ArgValue::Plain(s) => s.clone(),
                    other => panic!("unexpected {other:?}"),
                })
                .collect()
        };
        assert_eq!(
            plain(&a.game),
            [
                "--username",
                "${auth_player_name}",
                "--version",
                "${version_name}",
                "--tweakClass",
                "T"
            ]
        );
        assert_eq!(
            plain(&a.jvm),
            [
                "-Djava.library.path=${natives_directory}",
                "-cp",
                "${classpath}",
                "-Dx=1"
            ]
        );
    }

    #[test]
    fn legacy_parent_and_child_stay_legacy() {
        let merged = merge(
            version("1.12.2.json"),
            version("forge-1.12.2-14.23.5.2860-version.json"),
        );
        assert!(merged.arguments.is_none());
        assert!(merged
            .minecraft_arguments
            .unwrap()
            .contains("--tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker"));
    }
}
