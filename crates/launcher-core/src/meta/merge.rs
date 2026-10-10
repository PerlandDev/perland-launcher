use std::collections::HashSet;

use super::{
    maven::MavenCoord,
    mojang::{Arguments, VersionJson},
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
    let arguments = match (parent.arguments, child.arguments) {
        (Some(p), Some(c)) => Some(Arguments {
            game: [p.game, c.game].concat(),
            jvm: [p.jvm, c.jvm].concat(),
        }),
        (p, c) => c.or(p),
    };
    VersionJson {
        id: child.id,
        inherits_from: None,
        main_class: child.main_class,
        arguments,
        minecraft_arguments: child.minecraft_arguments.or(parent.minecraft_arguments),
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets),
        downloads: child.downloads.or(parent.downloads),
        java_version: child.java_version.or(parent.java_version),
        libraries,
        logging: child.logging.or(parent.logging),
        kind: child.kind.or(parent.kind),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{meta::mojang::ArgValue, test_support::version};

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
}
