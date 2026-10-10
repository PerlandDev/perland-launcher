# Forge and NeoForge (Plan 2 of 6) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `launcher-core` installs and launches Forge and NeoForge, including Asalith Fields' Forge 1.20.1-47.4.22. The work covers four parts:
- **Installer:** download the official installer jar and run its client-side processors.
- **Java:** find Java installations already on the computer.
- **Long command lines:** put Java's arguments in an argument file when the command line is too long.
- **Front ends:** offer Forge and NeoForge in the desktop app's New-instance picker and in the CLI.

**Architecture:**
- **Resolve.** `Installer::resolve` gets a Forge or NeoForge installer jar from maven, checked against the published `.sha1`. It reads `install_profile.json` and `version.json` from the jar, merges the version JSON onto vanilla, and returns the parsed profile in a new `ResolvedVersion::forge` field.
- **Install.** `Installer::install` runs the vanilla steps, then the loader steps:
  - It downloads the profile's libraries and extracts the jars bundled under the installer's `maven/` folder.
  - It runs the client-side processors with Mojang's Java for that Minecraft version, substituting the profile's data tokens and checking each output's sha1.
  - It copies the game jar to `versions/<forge-id>/<forge-id>.jar`, the name Forge's `-DignoreList` expects.
  - It writes a marker, so the next launch (online or offline) skips the processors.
- **Launch.** If the command line passes 30,000 characters and Java is 9 or newer, the JVM arguments move into a `@argfile`. The game arguments, which carry the access token, stay on the command line.

**Tech Stack:**
- Rust stable (edition 2021) with tokio, reqwest (rustls), serde/serde_json, zip 2 and thiserror.
- Tests use tempfile and wiremock.
- UI: React 19 + TypeScript, with Vitest and Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (§1.1, §1.3, §2, §2.2, §6, §7). The plan series table is in `docs/superpowers/plans/2026-10-09-launcher-core-foundation.md`.

**Owner decisions for this plan (2026-10-10):**
1. Forge and NeoForge get version lists in `meta::catalog` and are choosable in the desktop app's New-instance picker and instance settings.
2. Java detection is a core API, `java::detect_installed`. It is used automatically only when Mojang has no runtime for the computer, and only with an exact major-version match. The CLI also gets `java list`. No UI picker.

**Decisions this plan makes where the spec is silent:**
- **Processor Java.** Processors always run with Mojang's runtime for the version, even when the instance uses a custom Java. Mojang's runtime is known-good for the installer's tools.
- **Installer formats.** Only the installer format with a `json` field is supported: Forge 1.12.2-14.23.5.2851+ and every 1.13+ Forge and NeoForge. Older installers (with `versionInfo`) get a clear `LoaderTooOld` error.
- **Code origin.** The processor logic is written fresh from the installer's published `install_profile.json` format; nothing is copied from Modrinth's app-lib. `NOTICE` therefore stays unchanged. If an implementer does port app-lib code, that file gets the attribution header and a `NOTICE` entry.

## Global Constraints

- **License:** GPL-3.0-only. No Modrinth branding anywhere.
- **Dependencies:** `launcher-core` and `launcher-app` must not depend on Tauri or any UI crate. `src-tauri` is wiring only.
- **Platforms:** Rust stable, edition 2021. Must build and pass on **Windows x64 and Linux x64**.
- **Upstream URLs, exactly as in spec §2.2:**
  - Forge: `https://maven.minecraftforge.net`
  - NeoForge: `https://maven.neoforged.net/releases`
- **Errors:** player-facing failures are a `LauncherError` (category + stable `ErrorCode` + plain message + optional fix + detail + params). Every new `ErrorCode` gets `errors.<code>.message` in both `ui/src/i18n/en.json` and `ui/src/i18n/es.json` (neutral Latin American Spanish, informal "tú"). Then regenerate the contract: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit `ui/src/ipc/contract/*.json`.
- **Tokens:** tokens never reach logs or files. In particular, the access token never goes into the argfile.
- **Paths:** every path handed to Java is absolute (the game's working directory is the instance folder).
- **Remote strings:** when one becomes a path, it goes through `fsutil::safe_join` / `MavenCoord::parse` / `valid_component`.
- **Stable signatures:** keep the public signatures in `launch` and `accounts` stable. Other sessions work in this repo, so prefer additive changes. The one deliberate break is the new `ResolvedVersion::forge` field, and every construction site in this repo is updated in Task 6.
- **Heavy tests:** fuzzing, stress and load tests only run in CI, never on this PC. Local test inputs stay tiny. The real Forge install in Task 11 is a normal download.
- **Commits:** every commit message ends with the trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Before each commit:**
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - When `ui/` changed, also: `pnpm --filter ui build`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.
- **Machine notes:**
  - Windows Smart App Control sometimes blocks a fresh unsigned test exe (os error 4551). That's environmental; just retry.
  - `cargo` is at `%USERPROFILE%\.cargo\bin`.

## Review Focus

1. **Forge's `version.json` carries an empty `"logging": {}`.**
   - **Risk:** the current merge (`child.logging.or(parent.logging)`) would erase vanilla's log4j config. The game would then log with Forge's defaults and no `-Dlog4j.configurationFile`.
   - **Expected:** an empty child logging block keeps vanilla's.
   - **Pinned by:** `empty_child_logging_keeps_vanilla_logging` (Task 2).
2. **The game jar's file name versus Forge's `-DignoreList=…,${version_name}.jar`.**
   - **Risk:** BootstrapLauncher drops classpath entries whose file name starts with an ignore entry. If the vanilla jar stays at `versions/1.20.1/1.20.1.jar` while `${version_name}` is `1.20.1-forge-47.4.22`, the vanilla jar is loaded as a module next to Forge's patched one, and the game crashes at start.
   - **Expected:** for Forge and NeoForge, the classpath's game jar is `versions/<forge-id>/<forge-id>.jar`, the same layout as the official launcher.
   - **Pinned by:** `forge_classpath_names_the_game_jar_after_the_forge_version` (Task 6) and `forge_ignore_list_names_the_jar_on_the_classpath` (Task 7).
3. **The second launch, and an offline launch.**
   - **Risk:** processors take about a minute and need the network (`DOWNLOAD_MOJMAPS`). Rerunning them on every Play is slow, and offline it fails.
   - **Expected:** once a matching marker exists and every file the processors touch is still on disk, nothing reruns, and resolving works from the cache.
   - **Pinned by:** `current_install_skips_processors` (Task 6) and `installer_resolves_offline_after_the_first_download` (Task 6).
4. **The access token and the argfile.**
   - **Risk:** the argfile is written to disk under the instance folder.
   - **Expected:** only the JVM arguments (everything before the main class) go in the argfile. If a JVM argument somehow contains the token, no argfile is written at all.
   - **Pinned by:** `argfile_keeps_game_arguments_on_the_command_line` and `argfile_is_skipped_when_a_jvm_argument_holds_the_secret` (Task 7).
5. **A loader version picked for the wrong Minecraft version** (e.g. NeoForge 21.1.257, which is for 1.21.1, on a 1.21 instance).
   - **Expected:** a clear `LoaderVersionNotFound` before anything is installed. Not a broken install, and not a crash at launch.
   - **Pinned by:** `installer_for_another_minecraft_is_rejected` (Task 6).

---

### Task 1: Downloads: empty addresses and malformed addresses fail fast

Libraries that a loader installer builds locally have `"url": ""`. Today `Downloader::fetch` retries such an address 4 times with backoff, and it retries reqwest "builder" errors (a malformed URL) too. Neither will ever succeed.

**Files:**
- Modify: `crates/launcher-core/src/download/mod.rs` (`fetch`, `download_to_part`)
- Test: `crates/launcher-core/tests/download.rs`

**Interfaces:**
- Produces: `Downloader::fetch` skips blank URLs. With no usable URL it fails immediately with `ErrorCode::DownloadNoUrl`. A malformed URL moves straight on to the next URL (no retry).

- [ ] **Step 1: Write the failing tests** (append to `crates/launcher-core/tests/download.rs`, and add `ErrorCode` to the `launcher_core::{...}` import)

```rust
#[tokio::test]
async fn empty_url_fails_fast_without_retrying() {
    let dir = tempfile::tempdir().unwrap();
    let req = DownloadRequest::new(
        "",
        dir.path().join("forge-client.jar"),
        Hashes::sha1(HELLO_SHA1),
        Some(5),
    );
    // A 30 s backoff would blow the 5 s budget on the first retry.
    let slow = Downloader::new(reqwest::Client::new()).with_backoff(Duration::from_secs(30));
    let err = tokio::time::timeout(Duration::from_secs(5), slow.fetch(&req))
        .await
        .expect("an empty address must not be retried")
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::DownloadNoUrl);
}

#[tokio::test]
async fn malformed_url_moves_on_to_the_next_url() {
    let server = MockServer::start().await;
    serve(&server, "/hello.txt", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    let mut req = DownloadRequest::new("not a url", dest.clone(), Hashes::sha1(HELLO_SHA1), Some(5));
    req.urls.push(format!("{}/hello.txt", server.uri()));
    let slow = Downloader::new(reqwest::Client::new()).with_backoff(Duration::from_secs(30));
    tokio::time::timeout(Duration::from_secs(5), slow.fetch(&req))
        .await
        .expect("a malformed address must not be retried")
        .unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --test download -- empty_url malformed_url`
Expected: both FAIL with the timeout `expect` panics ("must not be retried").

- [ ] **Step 3: Implement.** In `fetch`, iterate only non-blank URLs. In `download_to_part`, turn a builder error into `Attempt::NextUrl`.

In `Downloader::fetch`, after `let part = part_path(&req.dest);`:

```rust
        // Libraries a loader installer builds itself have an empty URL: there is nothing to fetch.
        let urls: Vec<&String> = req.urls.iter().filter(|u| !u.trim().is_empty()).collect();
```

and change the loop header `'urls: for url in &req.urls {` to `'urls: for url in &urls {`.

In `download_to_part`, replace the `request.send().await.map_err(...)` closure with:

```rust
        let response = request.send().await.map_err(|e| {
            if e.is_builder() {
                // A malformed address won't get better by retrying it.
                return Attempt::NextUrl(
                    LauncherError::network(
                        ErrorCode::DownloadNoUrl,
                        "This file's download address isn't valid",
                    )
                    .with_detail(e),
                );
            }
            Attempt::Retry(
                LauncherError::network(
                    ErrorCode::HostUnreachable,
                    format!("Couldn't reach {}", host_of(url)),
                )
                .with_param("host", host_of(url))
                .with_fix("Check your internet connection")
                .with_detail(e),
            )
        })?;
```

- [ ] **Step 4: Run the download tests**

Run: `cargo test -p launcher-core --test download`
Expected: all PASS, including the existing `falls_back_to_next_url_on_404` and `retries_server_errors`.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/launcher-core/src/download/mod.rs crates/launcher-core/tests/download.rs
git commit -m "fix(core): don't retry empty or malformed download addresses" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Forge fixtures, locally-built libraries, merge fixes

This task covers three things:
- It records real installer metadata as fixtures.
- It plans libraries with an empty URL as "local": they stay on the classpath but are never downloaded.
- It fixes two merge gaps that Forge exposes: an empty child `logging`, and a legacy `minecraftArguments` parent under a modern `arguments` child.

**Files:**
- Create: `crates/launcher-core/tests/fixtures/forge-1.20.1-47.4.22-install_profile.json`, `forge-1.20.1-47.4.22-version.json`, `neoforge-21.1.257-install_profile.json`, `neoforge-21.1.257-version.json`, `forge-1.12.2-14.23.5.2860-install_profile.json`, `forge-1.12.2-14.23.5.2860-version.json`
- Modify: `crates/launcher-core/src/meta/mojang.rs` (add `LEGACY_JVM_ARGS`)
- Modify: `crates/launcher-core/src/meta/merge.rs`
- Modify: `crates/launcher-core/src/launch/mod.rs` (use `LEGACY_JVM_ARGS`)
- Modify: `crates/launcher-core/src/install/mod.rs` (`LibraryPlan::local`, `is_local`)

**Interfaces:**
- Produces:
  - `meta::mojang::LEGACY_JVM_ARGS: [&str; 3]`.
  - `install::LibraryPlan { downloads, natives, local: Vec<DownloadRequest> }`.
  - `install::is_local(&DownloadRequest) -> bool`, true when every URL is blank.
  - `merge` keeps the parent's logging when the child's has no `client`.
  - `merge` rewrites a legacy parent into modern `arguments` when the child is modern.

- [ ] **Step 1: Record the fixtures** (in Git Bash, from the repo root). The installers' sha1s were checked against maven on 2026-10-10.

```bash
F=crates/launcher-core/tests/fixtures
T=$(mktemp -d)
curl -sSfL -o "$T/forge.jar" https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-installer.jar
curl -sSfL -o "$T/neo.jar" https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.257/neoforge-21.1.257-installer.jar
curl -sSfL -o "$T/f1122.jar" https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860-installer.jar
sha1sum "$T"/*.jar
# expect afef69499ecb9c712e738d38aa679bd9ed1f3468 forge.jar, f088fccc003f4922d84f20e0dcbdaaa4c5d91f7e neo.jar, 5e8a91f71ef3d1f77de3f8d3261aedcc2f551c9d f1122.jar
unzip -p "$T/forge.jar" install_profile.json > $F/forge-1.20.1-47.4.22-install_profile.json
unzip -p "$T/forge.jar" version.json > $F/forge-1.20.1-47.4.22-version.json
unzip -p "$T/neo.jar" install_profile.json > $F/neoforge-21.1.257-install_profile.json
unzip -p "$T/neo.jar" version.json > $F/neoforge-21.1.257-version.json
unzip -p "$T/f1122.jar" install_profile.json > $F/forge-1.12.2-14.23.5.2860-install_profile.json
unzip -p "$T/f1122.jar" version.json > $F/forge-1.12.2-14.23.5.2860-version.json
sha1sum $F/forge-* $F/neoforge-*
rm -rf "$T"
```

Expected fixture sha1s:

| Fixture | sha1 |
|---|---|
| `forge-1.20.1-47.4.22-install_profile.json` | `20ec90f7b3567054424facf37f03598d7b99a9c6` |
| `forge-1.20.1-47.4.22-version.json` | `9250eec3b06ead221b445282e0427c55b693ae18` |
| `neoforge-21.1.257-install_profile.json` | `b12b1f15a5b1e620d51b3eb6e35b1600dab344bf` |
| `neoforge-21.1.257-version.json` | `6b66db2c17611ecffd908592d44f5ab56da9e74e` |
| `forge-1.12.2-14.23.5.2860-install_profile.json` | `5ffa3bee1468917598d347f046ded5a10aec60fb` |
| `forge-1.12.2-14.23.5.2860-version.json` | `6b48963265264172cee9296ba0ce3742f75edcc0` |

- [ ] **Step 2: Write the failing merge tests** (append inside `mod tests` in `crates/launcher-core/src/meta/merge.rs`)

```rust
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
```

- [ ] **Step 3: Write the failing library-plan test** (append inside `mod tests` in `crates/launcher-core/src/install/mod.rs`)

```rust
    #[test]
    fn libraries_without_a_url_are_local_not_downloaded() {
        let json = merge(
            version("1.12.2.json"),
            version("forge-1.12.2-14.23.5.2860-version.json"),
        );
        let ctx = RuleContext::new(WINDOWS_X64);
        let plan = plan_libraries(&json, &dirs(), &ctx).unwrap();
        assert_eq!(plan.local.len(), 1);
        let local = plan.local[0].dest.to_string_lossy().replace('\\', "/");
        assert!(
            local.ends_with("net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860.jar"),
            "{local}"
        );
        assert!(plan
            .downloads
            .iter()
            .all(|d| d.urls.iter().all(|u| !u.is_empty())));
        // The jar the installer provides is still on the classpath.
        let resolved = ResolvedVersion {
            vanilla_id: "1.12.2".into(),
            json,
        };
        let cp = classpath(&resolved, &dirs(), &ctx).unwrap();
        assert!(cp.contains(&plan.local[0].dest));
    }
```

- [ ] **Step 4: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib meta::merge install::tests::libraries_without`
Expected: compile error, because `plan.local` doesn't exist. After you comment out that test, the merge tests FAIL. `empty_child_logging_keeps_vanilla_logging` panics on `client.unwrap()`, and `legacy_parent_with_modern_child…` sees only `["--tweakClass","T"]`.

- [ ] **Step 5: Implement**

In `crates/launcher-core/src/meta/mojang.rs`, after `RESOURCES_URL`:

```rust
/// The JVM arguments every pre-1.13 version implies: its JSON only has `minecraftArguments`.
pub const LEGACY_JVM_ARGS: [&str; 3] = [
    "-Djava.library.path=${natives_directory}",
    "-cp",
    "${classpath}",
];
```

In `crates/launcher-core/src/launch/mod.rs`, import `meta::mojang::LEGACY_JVM_ARGS` and replace the inline fallback array in `build_command`:

```rust
        _ => args.extend(LEGACY_JVM_ARGS.iter().map(|t| substitute(t, &vars))),
```

Replace `crates/launcher-core/src/meta/merge.rs` above `#[cfg(test)]` with:

```rust
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
        game: legacy.split_whitespace().map(plain).chain(child.game).collect(),
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
```

Then change the test module's import to `use crate::test_support::version;`, because `ArgValue` now comes from `super::*`.

In `crates/launcher-core/src/install/mod.rs`:

```rust
#[derive(Debug, Default)]
pub struct LibraryPlan {
    pub downloads: Vec<DownloadRequest>,
    pub natives: Vec<NativeJar>,
    /// Libraries with no download address: the loader installer builds or bundles them.
    pub local: Vec<DownloadRequest>,
}

/// A library with no download address: a loader installer provides it, so it is never fetched.
pub fn is_local(req: &DownloadRequest) -> bool {
    req.urls.iter().all(|u| u.trim().is_empty())
}
```

and in `plan_libraries` replace

```rust
        if let Some(d) = library_artifact(lib, dirs)? {
            plan.downloads.push(d);
        }
```

with

```rust
        if let Some(d) = library_artifact(lib, dirs)? {
            if is_local(&d) {
                plan.local.push(d);
            } else {
                plan.downloads.push(d);
            }
        }
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p launcher-core --lib`
Expected: all PASS, including the existing `fabric_profile_merges_onto_vanilla`, `modern_command_on_windows` and `legacy_command_on_linux`.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/launcher-core
git commit -m "feat(core): Forge fixtures, locally built libraries, merge keeps vanilla logging and legacy arguments" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Java: safe runtime ids, installed-Java detection, fallback

**Files:**
- Modify: `crates/launcher-core/src/java.rs`
- Modify: `crates/launcher-core/src/play.rs` (`prepare` uses `ensure_java`)

**Interfaces:**
- Consumes: `meta::loaders::valid_component` (pub(crate)), `java::probe`.
- Produces:
  - `java::JavaInstall { path: PathBuf, major: u32 }` (Debug, Clone, PartialEq, Eq).
  - `java::candidate_paths(Platform, &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf>`.
  - `java::detect_installed(Platform) -> Vec<JavaInstall>` (async, newest major first).
  - `java::installed_fallback(LauncherError, &[JavaInstall], u32) -> Result<PathBuf>`.
  - `java::required_major(&VersionJson) -> u32`.
  - `java::ensure_java(meta, downloader, java_root, platform, &VersionJson, sink) -> Result<PathBuf>`.
  - `java::console_java(&Path) -> PathBuf`.
  - `ensure_runtime` now refuses a component id that isn't a plain segment, before any network call.

- [ ] **Step 1: Write the failing tests** (append inside `mod tests` in `crates/launcher-core/src/java.rs`)

```rust
    #[tokio::test]
    async fn unsafe_runtime_component_is_refused_before_any_download() {
        let tmp = tempfile::tempdir().unwrap();
        let meta = MetaClient::new(reqwest::Client::new(), tmp.path().join("meta"));
        let downloader = Downloader::new(reqwest::Client::new());
        for bad in ["../evil", "a/b", "", "..", "C:x"] {
            let err = ensure_runtime(
                &meta,
                &downloader,
                tmp.path(),
                WINDOWS_X64,
                bad,
                &crate::events::noop_sink(),
            )
            .await
            .unwrap_err();
            assert_eq!(err.code, crate::ErrorCode::UnsafeGameData, "{bad:?}");
        }
    }

    fn touch(root: &Path, rel: &str) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "").unwrap();
        p
    }

    #[test]
    fn finds_java_in_java_home_path_and_vendor_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let adoptium = touch(root, "pf/Eclipse Adoptium/jdk-17.0.9.9-hotspot/bin/javaw.exe");
        let home = touch(root, "home-java/bin/javaw.exe");
        let on_path = touch(root, "tools/bin/javaw.exe");
        touch(root, "pf/Eclipse Adoptium/not-a-jdk/readme.txt");
        let path_var =
            std::env::join_paths([root.join("tools/bin"), root.join("missing")]).unwrap();
        let env = |key: &str| match key {
            "ProgramFiles" => Some(root.join("pf").into_os_string()),
            "JAVA_HOME" => Some(root.join("home-java").into_os_string()),
            "PATH" => Some(path_var.clone()),
            _ => None,
        };
        assert_eq!(
            candidate_paths(WINDOWS_X64, &env),
            vec![home, on_path, adoptium]
        );
    }

    #[test]
    fn the_same_java_is_listed_once() {
        let tmp = tempfile::tempdir().unwrap();
        let java = touch(tmp.path(), "jdk/bin/java");
        let path_var = std::env::join_paths([tmp.path().join("jdk/bin")]).unwrap();
        let env = |key: &str| match key {
            "JAVA_HOME" => Some(tmp.path().join("jdk").into_os_string()),
            "PATH" => Some(path_var.clone()),
            _ => None,
        };
        let found: Vec<PathBuf> = candidate_paths(crate::test_support::LINUX_X64, &env)
            .into_iter()
            .filter(|p| p.starts_with(tmp.path()))
            .collect();
        assert_eq!(found, vec![java]);
    }

    #[test]
    fn installed_java_is_used_only_for_the_exact_major_version() {
        let installs = vec![
            JavaInstall {
                path: "/j21/bin/java".into(),
                major: 21,
            },
            JavaInstall {
                path: "/j17/bin/java".into(),
                major: 17,
            },
        ];
        let unavailable = || LauncherError::java(ErrorCode::JavaUnavailable, "none");
        assert_eq!(
            installed_fallback(unavailable(), &installs, 17).unwrap(),
            PathBuf::from("/j17/bin/java")
        );
        assert_eq!(
            installed_fallback(unavailable(), &installs, 8)
                .unwrap_err()
                .code,
            ErrorCode::JavaUnavailable
        );
    }

    #[test]
    fn required_major_comes_from_the_version() {
        assert_eq!(required_major(&version("1.20.1.json")), 17);
        assert_eq!(required_major(&version("1.12.2.json")), 8);
    }

    #[test]
    fn console_java_prefers_java_exe_next_to_javaw() {
        let tmp = tempfile::tempdir().unwrap();
        let javaw = touch(tmp.path(), "bin/javaw.exe");
        assert_eq!(console_java(&javaw), javaw, "no java.exe yet");
        let java = touch(tmp.path(), "bin/java.exe");
        assert_eq!(console_java(&javaw), java);
        let linux = tmp.path().join("bin/java");
        assert_eq!(console_java(&linux), linux);
    }
```

The test module also needs `use crate::{download::Downloader, meta::MetaClient, error::LauncherError};` next to its existing imports, if they aren't already in scope through `super::*`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib java::`
Expected: compile errors: `candidate_paths`, `JavaInstall`, `installed_fallback`, `required_major` and `console_java` are not found.

- [ ] **Step 3: Implement** (in `crates/launcher-core/src/java.rs`)

Add to the imports: `collections::HashSet`, `ffi::OsString`, `crate::meta::loaders::valid_component`, `futures_util::StreamExt`.

At the very top of `ensure_runtime`'s body:

```rust
    // `component` comes from the version JSON and becomes a folder name.
    if !valid_component(component) {
        return Err(LauncherError::integrity(
            ErrorCode::UnsafeGameData,
            "Minecraft's version data names an unsafe Java runtime",
        )
        .with_detail(component));
    }
```

Then append these items above `#[cfg(test)]`:

```rust
/// The Java major version a version asks for; versions without `javaVersion` predate Java 9.
pub fn required_major(version: &VersionJson) -> u32 {
    version
        .java_version
        .as_ref()
        .map_or(8, |j| j.major_version)
}

/// Mojang's runtime for `version`, or, when Mojang has none for this computer, an installed
/// Java of exactly the major version the game asks for.
pub async fn ensure_java(
    meta: &MetaClient,
    downloader: &Downloader,
    java_root: &Path,
    platform: Platform,
    version: &VersionJson,
    sink: &EventSink,
) -> Result<PathBuf> {
    match ensure_runtime(
        meta,
        downloader,
        java_root,
        platform,
        &component_for(version),
        sink,
    )
    .await
    {
        Err(e) if e.code == ErrorCode::JavaUnavailable => {
            let installs = detect_installed(platform).await;
            installed_fallback(e, &installs, required_major(version))
        }
        other => other,
    }
}

/// The installed Java with exactly `major`, else the original error. Only an exact match:
/// older Forge breaks on newer Java, and older Java can't run newer Minecraft.
pub fn installed_fallback(
    err: LauncherError,
    installs: &[JavaInstall],
    major: u32,
) -> Result<PathBuf> {
    installs
        .iter()
        .find(|j| j.major == major)
        .map(|j| j.path.clone())
        .ok_or(err)
}

/// A Java found on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInstall {
    pub path: PathBuf,
    pub major: u32,
}

/// Folders under Program Files that hold one folder per installed Java.
const WINDOWS_VENDORS: &[&str] = &[
    "Java",
    "Eclipse Adoptium",
    "Microsoft",
    "Zulu",
    "BellSoft",
    "Amazon Corretto",
    "Semeru",
    "Eclipse Foundation",
];

/// Java executables that exist on this computer: `JAVA_HOME`, then `PATH`, then the usual
/// install folders. Nothing is run here; each path appears once.
pub fn candidate_paths(platform: Platform, env: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let exe = match platform.os {
        Os::Windows => "javaw.exe",
        _ => "java",
    };
    let mut out = Vec::new();
    if let Some(home) = env("JAVA_HOME") {
        out.push(PathBuf::from(home).join("bin").join(exe));
    }
    if let Some(path) = env("PATH") {
        out.extend(std::env::split_paths(&path).map(|dir| dir.join(exe)));
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    match platform.os {
        Os::Windows => {
            for key in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
                if let Some(pf) = env(key) {
                    roots.extend(WINDOWS_VENDORS.iter().map(|v| PathBuf::from(&pf).join(v)));
                }
            }
            if let Some(profile) = env("USERPROFILE") {
                roots.push(PathBuf::from(profile).join(".jdks"));
            }
        }
        _ => {
            roots.extend(["/usr/lib/jvm", "/usr/lib64/jvm", "/opt/java"].map(PathBuf::from));
            if let Some(home) = env("HOME") {
                let home = PathBuf::from(home);
                roots.push(home.join(".jdks"));
                roots.push(home.join(".sdkman/candidates/java"));
            }
        }
    }
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut homes: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        homes.sort();
        out.extend(homes.into_iter().map(|h| h.join("bin").join(exe)));
    }
    let mut seen = HashSet::new();
    out.into_iter()
        .filter(|p| p.is_file() && seen.insert(p.clone()))
        .collect()
}

/// Every working Java on this computer, newest major version first. Each candidate is run once
/// (`-version`), at most four at a time.
pub async fn detect_installed(platform: Platform) -> Vec<JavaInstall> {
    let candidates =
        tokio::task::spawn_blocking(move || candidate_paths(platform, &|key| std::env::var_os(key)))
            .await
            .unwrap_or_default();
    let mut found: Vec<JavaInstall> = futures_util::stream::iter(candidates)
        .map(|path| async move { probe(&path).await.ok().map(|major| JavaInstall { path, major }) })
        .buffered(4)
        .filter_map(|j| async move { j })
        .collect()
        .await;
    found.sort_by(|a, b| b.major.cmp(&a.major));
    found
}

/// Mojang's runtime starts the game with `javaw.exe` (no console). Installer tools run better
/// with the `java.exe` next to it, whose output is captured the normal way.
pub fn console_java(java: &Path) -> PathBuf {
    let is_javaw = java
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe"));
    if is_javaw {
        let sibling = java.with_file_name("java.exe");
        if sibling.is_file() {
            return sibling;
        }
    }
    java.to_path_buf()
}
```

In `crates/launcher-core/src/play.rs`, `prepare`'s `None =>` arm becomes:

```rust
        None => {
            java::ensure_java(
                &installer.meta,
                &installer.downloader,
                &installer.dirs.java(),
                installer.platform,
                &resolved.json,
                sink,
            )
            .await?
        }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --lib java:: play::`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/launcher-core/src/java.rs crates/launcher-core/src/play.rs
git commit -m "feat(core): detect installed Java as a fallback; validate runtime component ids" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The Forge installer format: URLs, install profile, data values, tokens

**Files:**
- Create: `crates/launcher-core/src/meta/forge.rs`
- Modify: `crates/launcher-core/src/meta/mod.rs` (`pub mod forge;`)
- Modify: `crates/launcher-core/src/meta/loaders.rs` (extract `check_versions`; `map_profile_error` → `pub(crate)`)
- Modify: `crates/launcher-core/src/error.rs` (four new codes)
- Modify: `crates/launcher-core/src/test_support.rs` (`write_zip`)
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json` (new error strings and the `install_loader` progress label)
- Regenerate: `ui/src/ipc/contract/error_codes.json`

**Interfaces:**
- Consumes: `meta::maven::MavenCoord`, `meta::mojang::{Library, VersionJson}`, `meta::loaders::{LoaderKind, valid_component}`.
- Produces:
  - `ErrorCode::{LoaderInstallerInvalid, LoaderTooOld, LoaderProcessorFailed, LoaderOutputCorrupt}`.
  - `loaders::check_versions(mc, loader) -> Result<()>` and `loaders::map_profile_error` (both pub(crate)).
  - `meta::forge::{FORGE_MAVEN, NEOFORGE_MAVEN, MavenRepos<'a> { forge, neoforge }, OFFICIAL_REPOS}`.
  - `installer_coord(LoaderKind, &str, &str) -> Option<MavenCoord>`.
  - `installer_url(kind, mc, loader) -> Option<String>` and `installer_url_at(kind, mc, loader, MavenRepos) -> Option<String>`.
  - `InstallProfile { spec, minecraft, json, path, data: HashMap<String, SidedValue>, processors: Vec<Processor>, libraries: Vec<Library> }`.
  - `SidedValue { client, server }`.
  - `Processor { jar, classpath, args, outputs: BTreeMap<String,String>, sides }` with `runs_on_client()`.
  - `InstallerContents { profile, version }`.
  - `parse_profile(&[u8]) -> Result<InstallProfile>`.
  - `read_installer(&Path) -> Result<InstallerContents>` (blocking).
  - `read_entry(&mut ZipArchive<R>, &str, limit) -> Result<Vec<u8>>` (pub(crate)) and `invalid(detail) -> LauncherError` (pub(crate)).
  - `DataValue::{Artifact(MavenCoord), Literal(String), InstallerFile(String)}` with `parse_data_value(&str) -> Result<DataValue>`.
  - `replace_tokens(&str, &HashMap<String,String>) -> Result<String>`.
  - `test_support::write_zip(&Path, &[(&str, &[u8])])`.

- [ ] **Step 1: Add the error codes and their strings**

In `crates/launcher-core/src/error.rs`, extend the `// loaders and metadata` group:

```rust
    // loaders and metadata
    VersionInvalid, LoaderUnsupported, LoaderVersionNotFound, MetaCacheMissing, MetaNotFound,
    MetaChecksum, MetaUnexpected, LoaderInstallerInvalid, LoaderTooOld, LoaderProcessorFailed,
    LoaderOutputCorrupt,
```

In `ui/src/i18n/en.json`, inside `"errors"` (next to `loader_version_not_found`):

```json
    "loader_installer_invalid": { "message": "The mod loader's installer couldn't be read.", "fix": "Try again. If it keeps happening, pick another loader version." },
    "loader_too_old": { "message": "This loader version is too old for the launcher.", "fix": "Pick a newer loader version." },
    "loader_processor_failed": { "message": "Setting up the mod loader failed.", "fix": "Try again. If it keeps happening, copy the details and ask for help." },
    "loader_output_corrupt": { "message": "A file the mod loader built came out damaged.", "fix": "Try again." },
```

and inside `"progress"`:

```json
    "install_loader": "Setting up {{loader}} {{version}}"
```

In `ui/src/i18n/es.json`, in the same places:

```json
    "loader_installer_invalid": { "message": "No se pudo leer el instalador del loader.", "fix": "Inténtalo de nuevo. Si sigue pasando, elige otra versión del loader." },
    "loader_too_old": { "message": "Esta versión del loader es demasiado antigua para el launcher.", "fix": "Elige una versión más nueva del loader." },
    "loader_processor_failed": { "message": "No se pudo preparar el loader.", "fix": "Inténtalo de nuevo. Si sigue pasando, copia los detalles y pide ayuda." },
    "loader_output_corrupt": { "message": "Un archivo que generó el loader salió dañado.", "fix": "Inténtalo de nuevo." },
```

```json
    "install_loader": "Preparando {{loader}} {{version}}"
```

Regenerate the contract and check the UI's i18n test:

```bash
UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract
pnpm --filter ui test -- src/i18n
```

Expected: `ui/src/ipc/contract/error_codes.json` gains the four codes, and the i18n test PASSES.

- [ ] **Step 2: Write the failing tests** (new file `crates/launcher-core/src/meta/forge.rs`, tests only for now, plus `pub mod forge;` in `meta/mod.rs`)

Add to `crates/launcher-core/src/test_support.rs`:

```rust
/// Write a zip with the given entries. Generic over the body so `b"..."` literals work as-is
/// (an unsized coercion doesn't happen inside a tuple).
pub fn write_zip<B: AsRef<[u8]>>(path: &Path, entries: &[(&str, B)]) {
    let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, body) in entries {
        w.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut w, body.as_ref()).unwrap();
    }
    w.finish().unwrap();
}
```

Tests at the bottom of `meta/forge.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture, write_zip};

    #[test]
    fn installer_urls() {
        assert_eq!(
            installer_url(LoaderKind::Forge, "1.20.1", "47.4.22").unwrap(),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-installer.jar"
        );
        assert_eq!(
            installer_url(LoaderKind::NeoForge, "1.21.1", "21.1.257").unwrap(),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.257/neoforge-21.1.257-installer.jar"
        );
        // NeoForge's only 1.20.1 line kept Forge's coordinates.
        assert_eq!(
            installer_url(LoaderKind::NeoForge, "1.20.1", "47.1.106").unwrap(),
            "https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar"
        );
        assert!(installer_url(LoaderKind::Fabric, "1.20.1", "0.16.14").is_none());
        for (mc, loader) in [("1.20.1", "../x"), ("1.20.1/..", "1"), ("1.20.1", "")] {
            assert!(installer_url(LoaderKind::Forge, mc, loader).is_none(), "{mc} {loader}");
        }
        let repos = MavenRepos {
            forge: "http://127.0.0.1:9/",
            neoforge: "http://127.0.0.1:8",
        };
        assert_eq!(
            installer_url_at(LoaderKind::Forge, "1.20.1", "47.4.22", repos).unwrap(),
            "http://127.0.0.1:9/net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-installer.jar"
        );
    }

    #[test]
    fn forge_1_20_1_profile_parses() {
        let p = parse_profile(fixture("forge-1.20.1-47.4.22-install_profile.json").as_bytes())
            .unwrap();
        assert_eq!(p.spec, 1);
        assert_eq!(p.minecraft, "1.20.1");
        assert_eq!(p.json, "/version.json");
        assert_eq!(p.libraries.len(), 36);
        assert_eq!(p.processors.len(), 10);
        assert_eq!(p.processors.iter().filter(|x| x.runs_on_client()).count(), 6);
        assert_eq!(
            p.data["MC_SLIM_SHA"].client,
            "'de86b035d2da0f78940796bb95c39a932ed84834'"
        );
        let split = &p.processors[6];
        assert_eq!(split.jar, "net.minecraftforge:jarsplitter:1.1.4");
        assert_eq!(split.outputs["{MC_SLIM}"], "{MC_SLIM_SHA}");
    }

    #[test]
    fn neoforge_profile_has_fat_jars_and_no_outputs() {
        let p = parse_profile(fixture("neoforge-21.1.257-install_profile.json").as_bytes()).unwrap();
        assert_eq!(p.minecraft, "1.21.1");
        assert_eq!(p.libraries.len(), 70);
        let client: Vec<&Processor> = p.processors.iter().filter(|x| x.runs_on_client()).collect();
        assert_eq!(client.len(), 6);
        assert!(client.iter().all(|x| x.classpath.is_empty() && x.outputs.is_empty()));
    }

    #[test]
    fn forge_1_12_2_profile_names_its_bundled_jar() {
        let p = parse_profile(fixture("forge-1.12.2-14.23.5.2860-install_profile.json").as_bytes())
            .unwrap();
        assert_eq!(p.path.as_deref(), Some("net.minecraftforge:forge:1.12.2-14.23.5.2860"));
        assert!(p.processors.is_empty());
    }

    #[test]
    fn legacy_installer_format_is_too_old() {
        let err = parse_profile(br#"{"install":{"target":"x"},"versionInfo":{"id":"x"}}"#)
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::LoaderTooOld);
        let garbage = parse_profile(b"<html>").unwrap_err();
        assert_eq!(garbage.code, ErrorCode::LoaderInstallerInvalid);
    }

    #[test]
    fn reads_profile_and_version_from_an_installer_jar() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("installer.jar");
        let profile = fixture("forge-1.20.1-47.4.22-install_profile.json");
        let version = fixture("forge-1.20.1-47.4.22-version.json");
        write_zip(
            &jar,
            &[
                ("install_profile.json", profile.as_bytes()),
                ("version.json", version.as_bytes()),
            ],
        );
        let contents = read_installer(&jar).unwrap();
        assert_eq!(contents.version.id, "1.20.1-forge-47.4.22");
        assert_eq!(contents.version.inherits_from.as_deref(), Some("1.20.1"));
        assert_eq!(contents.profile.processors.len(), 10);

        let broken = tmp.path().join("broken.jar");
        write_zip(&broken, &[("install_profile.json", profile.as_bytes())]);
        assert_eq!(
            read_installer(&broken).unwrap_err().code,
            ErrorCode::LoaderInstallerInvalid
        );
    }

    #[test]
    fn oversized_entries_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("x.jar");
        write_zip(&jar, &[("a.json", b"0123456789")]);
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&jar).unwrap()).unwrap();
        assert_eq!(read_entry(&mut archive, "a.json", 10).unwrap(), b"0123456789");
        assert_eq!(
            read_entry(&mut archive, "a.json", 4).unwrap_err().code,
            ErrorCode::LoaderInstallerInvalid
        );
    }

    #[test]
    fn data_values_like_the_forge_installer() {
        assert_eq!(
            parse_data_value("[net.minecraft:client:1.20.1-20230612.114412:srg]").unwrap(),
            DataValue::Artifact(
                MavenCoord::parse("net.minecraft:client:1.20.1-20230612.114412:srg").unwrap()
            )
        );
        assert_eq!(
            parse_data_value("'de86b035'").unwrap(),
            DataValue::Literal("de86b035".into())
        );
        assert_eq!(
            parse_data_value("/data/client.lzma").unwrap(),
            DataValue::InstallerFile("data/client.lzma".into())
        );
        assert_eq!(
            parse_data_value("[../../evil]").unwrap_err().code,
            ErrorCode::UnsafeGameData
        );
    }

    #[test]
    fn tokens_like_the_forge_installer() {
        let data = HashMap::from([
            ("SIDE".to_string(), "client".to_string()),
            ("MC_SLIM".to_string(), "/l/slim.jar".to_string()),
        ]);
        assert_eq!(replace_tokens("{SIDE}", &data).unwrap(), "client");
        assert_eq!(replace_tokens("--x={MC_SLIM}", &data).unwrap(), "--x=/l/slim.jar");
        assert_eq!(replace_tokens("'{SIDE}'", &data).unwrap(), "{SIDE}");
        assert_eq!(replace_tokens(r"a\{b", &data).unwrap(), "a{b");
        assert_eq!(replace_tokens("plain", &data).unwrap(), "plain");
        for bad in ["{NOPE}", "{SIDE", "'open", "x\\"] {
            assert_eq!(
                replace_tokens(bad, &data).unwrap_err().code,
                ErrorCode::LoaderInstallerInvalid,
                "{bad}"
            );
        }
    }
}
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib meta::forge`
Expected: compile errors, because none of the items exist yet.

- [ ] **Step 4: Implement**

In `crates/launcher-core/src/meta/loaders.rs`, pull the validation loop out of `fetch_profile`. It now calls `check_versions(mc, loader)?;` instead. Also make `map_profile_error` `pub(crate)`.

```rust
/// Both version strings must be safe to put in a URL path.
pub(crate) fn check_versions(mc: &str, loader: &str) -> Result<()> {
    for value in [mc, loader] {
        if !valid_component(value) {
            return Err(LauncherError::loader(
                ErrorCode::VersionInvalid,
                format!("\"{value}\" isn't a valid version"),
            )
            .with_param("value", value)
            .with_fix("Pick a version from the list instead of typing one in"));
        }
    }
    Ok(())
}
```

`crates/launcher-core/src/meta/forge.rs` (above the tests):

```rust
//! Forge and NeoForge ship an installer jar instead of a launch profile. This module knows where
//! the installer lives and how to read it: `install_profile.json` (libraries, plus the processors
//! that build the patched game) and the version JSON that inherits from vanilla. It follows the
//! installer's published format; nothing here is ported from another launcher.
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
    io::{Read, Seek},
    path::Path,
};

use serde::Deserialize;

use super::{
    loaders::{valid_component, LoaderKind},
    maven::MavenCoord,
    mojang::{Library, VersionJson},
};
use crate::error::{ErrorCode, LauncherError, Result};

pub const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";

/// The maven repositories installers and version lists come from (tests point them elsewhere).
#[derive(Debug, Clone, Copy)]
pub struct MavenRepos<'a> {
    pub forge: &'a str,
    pub neoforge: &'a str,
}

pub const OFFICIAL_REPOS: MavenRepos<'static> = MavenRepos {
    forge: FORGE_MAVEN,
    neoforge: NEOFORGE_MAVEN,
};

/// JSON inside an installer is small; anything bigger isn't an installer.
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;

/// The installer's maven coordinate. `None` for Fabric/Quilt or version strings unsafe for a path.
pub fn installer_coord(kind: LoaderKind, mc: &str, loader: &str) -> Option<MavenCoord> {
    if !valid_component(mc) || !valid_component(loader) {
        return None;
    }
    let name = match kind {
        LoaderKind::Forge => format!("net.minecraftforge:forge:{mc}-{loader}:installer"),
        // NeoForge's only 1.20.1 line kept Forge's coordinates.
        LoaderKind::NeoForge if mc == "1.20.1" => {
            format!("net.neoforged:forge:{mc}-{loader}:installer")
        }
        LoaderKind::NeoForge => format!("net.neoforged:neoforge:{loader}:installer"),
        LoaderKind::Fabric | LoaderKind::Quilt => return None,
    };
    MavenCoord::parse(&name)
}

pub fn installer_url(kind: LoaderKind, mc: &str, loader: &str) -> Option<String> {
    installer_url_at(kind, mc, loader, OFFICIAL_REPOS)
}

pub fn installer_url_at(
    kind: LoaderKind,
    mc: &str,
    loader: &str,
    repos: MavenRepos,
) -> Option<String> {
    let base = match kind {
        LoaderKind::Forge => repos.forge,
        _ => repos.neoforge,
    };
    installer_coord(kind, mc, loader)
        .map(|c| format!("{}/{}", base.trim_end_matches('/'), c.path()))
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InstallProfile {
    #[serde(default)]
    pub spec: u32,
    pub minecraft: String,
    /// Where the version JSON sits inside the installer, e.g. `/version.json`.
    pub json: String,
    /// A maven coordinate whose jar is bundled under the installer's `maven/` folder (1.12.2).
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub data: HashMap<String, SidedValue>,
    #[serde(default)]
    pub processors: Vec<Processor>,
    #[serde(default)]
    pub libraries: Vec<Library>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SidedValue {
    pub client: String,
    #[serde(default)]
    pub server: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Processor {
    pub jar: String,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Output file → expected sha1, both written with `{TOKENS}`.
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
    /// `None` means both sides.
    #[serde(default)]
    pub sides: Option<Vec<String>>,
}

impl Processor {
    pub fn runs_on_client(&self) -> bool {
        self.sides
            .as_ref()
            .is_none_or(|s| s.iter().any(|x| x == "client"))
    }
}

#[derive(Debug, Clone)]
pub struct InstallerContents {
    pub profile: InstallProfile,
    pub version: VersionJson,
}

pub(crate) fn invalid(detail: impl Display) -> LauncherError {
    LauncherError::loader(
        ErrorCode::LoaderInstallerInvalid,
        "The mod loader's installer couldn't be read",
    )
    .with_fix("Try again. If it keeps happening, pick another loader version.")
    .with_detail(detail)
}

/// Read one zip entry, refusing entries larger than `limit` bytes.
pub(crate) fn read_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>> {
    let entry = archive
        .by_name(name)
        .map_err(|e| invalid(format!("{name}: {e}")))?;
    let mut bytes = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| invalid(format!("{name}: {e}")))?;
    if bytes.len() as u64 > limit {
        return Err(invalid(format!("{name} is too large")));
    }
    Ok(bytes)
}

/// Installers from before Forge 1.12.2-14.23.5.2851 use another format (`install` + `versionInfo`).
pub fn parse_profile(bytes: &[u8]) -> Result<InstallProfile> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| invalid(format!("install_profile.json: {e}")))?;
    if value.get("versionInfo").is_some() {
        return Err(LauncherError::loader(
            ErrorCode::LoaderTooOld,
            "This loader version is too old for the launcher",
        )
        .with_fix("Pick a newer loader version")
        .with_detail("legacy installer format (versionInfo)"));
    }
    serde_json::from_value(value).map_err(|e| invalid(format!("install_profile.json: {e}")))
}

/// Read the install profile and the version JSON it names. Blocking: run it on `spawn_blocking`.
pub fn read_installer(jar: &Path) -> Result<InstallerContents> {
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(jar)?).map_err(invalid)?;
    let profile = parse_profile(&read_entry(&mut archive, "install_profile.json", MAX_JSON_BYTES)?)?;
    let version_bytes = read_entry(&mut archive, profile.json.trim_start_matches('/'), MAX_JSON_BYTES)?;
    let version: VersionJson = serde_json::from_slice(&version_bytes)
        .map_err(|e| invalid(format!("{}: {e}", profile.json)))?;
    Ok(InstallerContents { profile, version })
}

/// One install-profile data value, read the way the Forge installer reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataValue {
    /// `[group:artifact:version]`: a file in the libraries folder.
    Artifact(MavenCoord),
    /// `'text'`: the text itself.
    Literal(String),
    /// Anything else (e.g. `/data/client.lzma`): a file inside the installer jar.
    InstallerFile(String),
}

pub fn parse_data_value(raw: &str) -> Result<DataValue> {
    if let Some(inner) = raw.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return MavenCoord::parse(inner)
            .map(DataValue::Artifact)
            .ok_or_else(|| unsafe_value(raw));
    }
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return Ok(DataValue::Literal(raw[1..raw.len() - 1].to_string()));
    }
    Ok(DataValue::InstallerFile(raw.trim_start_matches('/').to_string()))
}

pub(crate) fn unsafe_value(raw: &str) -> LauncherError {
    LauncherError::integrity(
        ErrorCode::UnsafeGameData,
        "The mod loader's installer contains an unsafe path",
    )
    .with_detail(raw)
}

/// Replace `{KEY}` with `data[KEY]` and unwrap `'quoted'` text, with `\` escaping the next
/// character: how the Forge installer reads processor arguments. An unknown key is an error.
pub fn replace_tokens(value: &str, data: &HashMap<String, String>) -> Result<String> {
    let dangling = || invalid(format!("dangling escape in {value}"));
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next().ok_or_else(dangling)?),
            '{' | '\'' => {
                let close = if c == '{' { '}' } else { '\'' };
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        None => return Err(invalid(format!("unclosed {c} in {value}"))),
                        Some('\\') => inner.push(chars.next().ok_or_else(dangling)?),
                        Some(d) if d == close => break,
                        Some(d) => inner.push(d),
                    }
                }
                if c == '{' {
                    let found = data
                        .get(&inner)
                        .ok_or_else(|| invalid(format!("unknown key {inner} in {value}")))?;
                    out.push_str(found);
                } else {
                    out.push_str(&inner);
                }
            }
            _ => out.push(c),
        }
    }
    Ok(out)
}
```

`Option::is_none_or` needs Rust 1.82+; the toolchain is 1.99. If clippy suggests `map_or(true, …)` instead, follow clippy.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p launcher-core --lib meta::`
Expected: all PASS, including the `loaders` tests (`fetch_profile_distinguishes_invalid_from_unsupported` still sees "isn't a valid version").

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm --filter ui test
git add crates/launcher-core ui/src/i18n ui/src/ipc/contract
git commit -m "feat(core): read Forge and NeoForge installers (profile, data values, tokens)" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Planning and running the client processors

**Files:**
- Create: `crates/launcher-core/src/install/forge.rs`
- Modify: `crates/launcher-core/src/install/mod.rs` (`pub mod forge;`)

**Interfaces:**
- Consumes:
  - `meta::forge::{InstallProfile, Processor, parse_data_value, DataValue, replace_tokens, read_entry, invalid, unsafe_value}`.
  - `fsutil::safe_join`, `download::Hashes`.
- Produces:
  - `install::forge::DataContext<'a> { libraries, installer, extract_dir, minecraft_jar, minecraft, root }`.
  - `ProcessorData { values: HashMap<String,String>, artifacts: HashSet<String>, extract: Vec<(String, PathBuf)> }`.
  - `resolve_data(&InstallProfile, &DataContext) -> Result<ProcessorData>`.
  - `ProcessorCall { classpath: Vec<PathBuf>, args: Vec<String>, outputs: Vec<(PathBuf, String)>, files: Vec<PathBuf> }`.
  - `plan_processors(&InstallProfile, &ProcessorData, libraries: &Path) -> Result<Vec<ProcessorCall>>`.
  - `main_class(&Path) -> Result<String>` (blocking).
  - `java_args(&ProcessorCall, main_class, separator) -> Vec<String>`.
  - `outputs_current(&ProcessorCall) -> bool` (async) and `verify_outputs(&ProcessorCall) -> Result<()>` (async).
  - `run_processor(java, &ProcessorCall, separator, cwd, timeout) -> Result<()>` (async).
  - `PROCESSOR_TIMEOUT`.

- [ ] **Step 1: Write the failing tests** (in the new `crates/launcher-core/src/install/forge.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        meta::forge::parse_profile,
        paths::LauncherDirs,
        test_support::{fixture, write_zip},
        ErrorCode,
    };

    const HELLO_SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

    fn arg_after<'a>(args: &'a [String], flag: &str) -> &'a str {
        let i = args.iter().position(|a| a == flag).unwrap_or_else(|| panic!("{flag}"));
        &args[i + 1]
    }

    struct Paths {
        libs: PathBuf,
        installer: PathBuf,
        extract: PathBuf,
        mc_jar: PathBuf,
        root: PathBuf,
    }

    fn paths() -> Paths {
        let dirs = LauncherDirs::new("/pl");
        let libs = dirs.libraries();
        Paths {
            installer: libs.join("net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-installer.jar"),
            extract: dirs.version_dir("1.20.1-forge-47.4.22").join(".installer"),
            mc_jar: dirs.client_jar("1.20.1"),
            root: dirs.root().to_path_buf(),
            libs,
        }
    }

    fn plan(profile_fixture: &str, minecraft: &str) -> (Paths, ProcessorData, Vec<ProcessorCall>) {
        let p = paths();
        let profile = parse_profile(fixture(profile_fixture).as_bytes()).unwrap();
        let data = resolve_data(
            &profile,
            &DataContext {
                libraries: &p.libs,
                installer: &p.installer,
                extract_dir: &p.extract,
                minecraft_jar: &p.mc_jar,
                minecraft,
                root: &p.root,
            },
        )
        .unwrap();
        let calls = plan_processors(&profile, &data, &p.libs).unwrap();
        (p, data, calls)
    }

    #[test]
    fn forge_client_processors_are_planned_with_real_paths() {
        let (p, data, calls) = plan("forge-1.20.1-47.4.22-install_profile.json", "1.20.1");
        let lib = |rel: &str| path_string(&p.libs.join(rel));
        assert_eq!(
            data.extract,
            vec![("data/client.lzma".to_string(), p.extract.join("data/client.lzma"))]
        );
        assert_eq!(data.values["SIDE"], "client");
        assert_eq!(calls.len(), 6);
        // MCP_DATA reads the mcp_config zip named inline as [coordinate].
        assert_eq!(
            arg_after(&calls[0].args, "--input"),
            lib("de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip")
        );
        assert_eq!(arg_after(&calls[1].args, "--side"), "client");
        let split = &calls[3];
        assert_eq!(split.classpath.len(), 3);
        assert_eq!(arg_after(&split.args, "--input"), path_string(&p.mc_jar));
        assert_eq!(
            split.outputs,
            vec![
                (
                    p.libs.join("net/minecraft/client/1.20.1-20230612.114412/client-1.20.1-20230612.114412-extra.jar"),
                    "8c5a95cbce940cfdb304376ae9fea47968d02587".to_string()
                ),
                (
                    p.libs.join("net/minecraft/client/1.20.1-20230612.114412/client-1.20.1-20230612.114412-slim.jar"),
                    "de86b035d2da0f78940796bb95c39a932ed84834".to_string()
                ),
            ]
        );
        let patch = &calls[5];
        let patched = "net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-client.jar";
        assert_eq!(arg_after(&patch.args, "--apply"), path_string(&p.extract.join("data/client.lzma")));
        assert_eq!(arg_after(&patch.args, "--output"), lib(patched));
        assert!(patch.files.contains(&p.libs.join(patched)));
        assert!(
            !calls.iter().flat_map(|c| &c.args).any(|a| a.contains('{')),
            "every token is resolved"
        );
    }

    #[test]
    fn neoforge_processors_are_single_fat_jars() {
        let (_, _, calls) = plan("neoforge-21.1.257-install_profile.json", "1.21.1");
        assert_eq!(calls.len(), 6);
        assert!(calls.iter().all(|c| c.classpath.len() == 1 && c.outputs.is_empty()));
        assert!(calls[5].classpath[0].ends_with("binarypatcher-2.1.2-fatjar.jar"));
    }

    #[test]
    fn processor_paths_cannot_escape_the_libraries_folder() {
        let p = paths();
        let mut profile =
            parse_profile(fixture("forge-1.20.1-47.4.22-install_profile.json").as_bytes()).unwrap();
        let data = ProcessorData::default();
        profile.processors = vec![Processor {
            jar: "a:b:../../evil".into(),
            classpath: vec![],
            args: vec![],
            outputs: Default::default(),
            sides: None,
        }];
        assert_eq!(
            plan_processors(&profile, &data, &p.libs).unwrap_err().code,
            ErrorCode::UnsafeGameData
        );
        profile.processors[0].jar = "a:b:1".into();
        profile.processors[0].outputs = [("/etc/passwd".to_string(), format!("'{HELLO_SHA1}'"))].into();
        assert_eq!(
            plan_processors(&profile, &data, &p.libs).unwrap_err().code,
            ErrorCode::UnsafeGameData
        );
    }

    #[test]
    fn main_class_comes_from_the_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("tool.jar");
        write_zip(
            &jar,
            &[(
                "META-INF/MANIFEST.MF",
                b"Manifest-Version: 1.0\r\nMain-Class: net.minecraftforge.jarsplitter.ConsoleTool\r\n\r\n",
            )],
        );
        assert_eq!(main_class(&jar).unwrap(), "net.minecraftforge.jarsplitter.ConsoleTool");
        let bare = tmp.path().join("bare.jar");
        write_zip(&bare, &[("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\r\n")]);
        assert_eq!(main_class(&bare).unwrap_err().code, ErrorCode::LoaderInstallerInvalid);
    }

    #[tokio::test]
    async fn outputs_are_checked_and_bad_ones_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("slim.jar");
        std::fs::write(&out, "hello").unwrap();
        let mut call = ProcessorCall {
            classpath: vec![tmp.path().join("tool.jar")],
            args: vec![],
            outputs: vec![(out.clone(), HELLO_SHA1.to_string())],
            files: vec![],
        };
        assert!(outputs_current(&call).await);
        verify_outputs(&call).await.unwrap();
        call.outputs[0].1 = "0".repeat(40);
        assert!(!outputs_current(&call).await);
        assert_eq!(
            verify_outputs(&call).await.unwrap_err().code,
            ErrorCode::LoaderOutputCorrupt
        );
        assert!(!out.exists(), "a damaged output is removed");
        call.outputs.clear();
        assert!(!outputs_current(&call).await, "no outputs means it must run");
    }

    #[tokio::test]
    async fn a_failing_processor_reports_its_output() {
        // Skipped where no java is installed.
        if std::process::Command::new("java").arg("-version").output().is_err() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("tool.jar");
        write_zip(&jar, &[("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\nMain-Class: perland.Nope\n")]);
        let call = ProcessorCall {
            classpath: vec![jar],
            args: vec!["--task".into(), "X".into()],
            outputs: vec![],
            files: vec![],
        };
        let sep = if cfg!(windows) { ";" } else { ":" };
        let err = run_processor(Path::new("java"), &call, sep, tmp.path(), Duration::from_secs(60))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::LoaderProcessorFailed);
        assert_eq!(err.params["step"], "tool.jar");
        assert!(err.detail.unwrap().contains("perland.Nope"));
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib install::forge`
Expected: compile errors, because the items don't exist.

- [ ] **Step 3: Implement** (top of `crates/launcher-core/src/install/forge.rs`)

```rust
//! Running a Forge or NeoForge installer's client-side processors: the Java tools that turn the
//! vanilla jar into the patched game Forge loads. Written from the installer's published
//! `install_profile.json` format. Every output with a published sha1 is checked.
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use crate::{
    download::Hashes,
    error::{ErrorCode, LauncherError, Result},
    fsutil::safe_join,
    meta::{
        forge::{
            invalid, parse_data_value, read_entry, replace_tokens, unsafe_value, DataValue,
            InstallProfile, Processor,
        },
        maven::MavenCoord,
    },
};

/// One processor may take minutes on a slow disk; past this it's stuck.
pub const PROCESSOR_TIMEOUT: Duration = Duration::from_secs(600);
/// Lines of a failed processor's output kept for the error details.
const OUTPUT_TAIL: usize = 30;

/// The paths the profile's built-in data keys stand for.
pub struct DataContext<'a> {
    pub libraries: &'a Path,
    pub installer: &'a Path,
    /// Where installer files named by data values (e.g. `data/client.lzma`) are extracted.
    pub extract_dir: &'a Path,
    pub minecraft_jar: &'a Path,
    pub minecraft: &'a str,
    pub root: &'a Path,
}

#[derive(Debug, Default)]
pub struct ProcessorData {
    pub values: HashMap<String, String>,
    /// Values that name a library file, so a plan can tell which arguments are files.
    pub artifacts: HashSet<String>,
    /// Installer entries to extract before the processors run: (entry, destination), sorted.
    pub extract: Vec<(String, PathBuf)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessorCall {
    /// The processor jar first, then its classpath.
    pub classpath: Vec<PathBuf>,
    pub args: Vec<String>,
    /// Files the processor must produce, with their expected sha1 (lowercase).
    pub outputs: Vec<(PathBuf, String)>,
    /// Every library file the call reads or writes; all of them exist after a good install.
    pub files: Vec<PathBuf>,
}

fn path_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn coord_path(libraries: &Path, name: &str) -> Result<PathBuf> {
    let coord = MavenCoord::parse(name).ok_or_else(|| unsafe_value(name))?;
    safe_join(libraries, &coord.path()).ok_or_else(|| unsafe_value(name))
}

/// The profile's data with every value resolved for the client side, plus the built-in keys.
pub fn resolve_data(profile: &InstallProfile, ctx: &DataContext) -> Result<ProcessorData> {
    let mut data = ProcessorData::default();
    for (key, sided) in &profile.data {
        let value = match parse_data_value(&sided.client)? {
            DataValue::Artifact(coord) => {
                let path = safe_join(ctx.libraries, &coord.path())
                    .ok_or_else(|| unsafe_value(&sided.client))?;
                let path = path_string(&path);
                data.artifacts.insert(path.clone());
                path
            }
            DataValue::Literal(text) => text,
            DataValue::InstallerFile(entry) => {
                let dest = safe_join(ctx.extract_dir, &entry).ok_or_else(|| unsafe_value(&entry))?;
                data.extract.push((entry, dest.clone()));
                path_string(&dest)
            }
        };
        data.values.insert(key.clone(), value);
    }
    for (key, value) in [
        ("SIDE", "client".to_string()),
        ("MINECRAFT_JAR", path_string(ctx.minecraft_jar)),
        ("MINECRAFT_VERSION", ctx.minecraft.to_string()),
        ("ROOT", path_string(ctx.root)),
        ("INSTALLER", path_string(ctx.installer)),
        ("LIBRARY_DIR", path_string(ctx.libraries)),
    ] {
        data.values.insert(key.to_string(), value);
    }
    data.extract.sort();
    Ok(data)
}

/// The client-side processors, in profile order, with every argument resolved.
pub fn plan_processors(
    profile: &InstallProfile,
    data: &ProcessorData,
    libraries: &Path,
) -> Result<Vec<ProcessorCall>> {
    profile
        .processors
        .iter()
        .filter(|p| p.runs_on_client())
        .map(|p| plan_one(p, data, libraries))
        .collect()
}

fn plan_one(p: &Processor, data: &ProcessorData, libraries: &Path) -> Result<ProcessorCall> {
    let mut classpath = vec![coord_path(libraries, &p.jar)?];
    for name in &p.classpath {
        classpath.push(coord_path(libraries, name)?);
    }
    let mut files = classpath.clone();
    let mut args = Vec::with_capacity(p.args.len());
    for arg in &p.args {
        let value = match arg.strip_prefix('[').and_then(|a| a.strip_suffix(']')) {
            Some(name) => {
                let path = coord_path(libraries, name)?;
                files.push(path.clone());
                path_string(&path)
            }
            None => {
                let value = replace_tokens(arg, &data.values)?;
                if data.artifacts.contains(&value) {
                    files.push(PathBuf::from(&value));
                }
                value
            }
        };
        args.push(value);
    }
    let mut outputs = Vec::with_capacity(p.outputs.len());
    for (key, sha) in &p.outputs {
        let path = PathBuf::from(replace_tokens(key, &data.values)?);
        // Outputs are checked and, when damaged, deleted: they must be library files.
        if !path.starts_with(libraries) {
            return Err(unsafe_value(key));
        }
        let sha1 = replace_tokens(sha, &data.values)?.to_ascii_lowercase();
        if sha1.len() != 40 || !sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid(format!("bad output checksum {sha}")));
        }
        outputs.push((path, sha1));
    }
    Ok(ProcessorCall {
        classpath,
        args,
        outputs,
        files,
    })
}

/// `Main-Class` from a jar's manifest. Blocking.
pub fn main_class(jar: &Path) -> Result<String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?)
        .map_err(|e| invalid(format!("{}: {e}", jar.display())))?;
    let manifest = read_entry(&mut archive, "META-INF/MANIFEST.MF", 1024 * 1024)?;
    String::from_utf8_lossy(&manifest)
        .lines()
        .find_map(|l| l.strip_prefix("Main-Class:"))
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .ok_or_else(|| invalid(format!("{} has no Main-Class", jar.display())))
}

pub fn java_args(call: &ProcessorCall, main_class: &str, separator: &str) -> Vec<String> {
    let cp = call
        .classpath
        .iter()
        .map(|p| path_string(p))
        .collect::<Vec<_>>()
        .join(separator);
    let mut args = vec!["-cp".to_string(), cp, main_class.to_string()];
    args.extend(call.args.iter().cloned());
    args
}

async fn file_matches(path: &Path, sha1: &str) -> bool {
    let (path, hashes) = (path.to_path_buf(), Hashes::sha1(sha1));
    tokio::task::spawn_blocking(move || path.is_file() && hashes.verify_file(&path).unwrap_or(false))
        .await
        .unwrap_or(false)
}

/// True when the call has outputs and every one is on disk with its expected sha1, so the call
/// can be skipped. A call without outputs always runs.
pub async fn outputs_current(call: &ProcessorCall) -> bool {
    if call.outputs.is_empty() {
        return false;
    }
    for (path, sha1) in &call.outputs {
        if !file_matches(path, sha1).await {
            return false;
        }
    }
    true
}

pub async fn verify_outputs(call: &ProcessorCall) -> Result<()> {
    for (path, sha1) in &call.outputs {
        if !file_matches(path, sha1).await {
            let _ = tokio::fs::remove_file(path).await;
            return Err(LauncherError::integrity(
                ErrorCode::LoaderOutputCorrupt,
                "A file the mod loader built came out damaged",
            )
            .with_fix("Try again")
            .with_detail(format!("{} (expected sha1 {sha1})", path.display())));
        }
    }
    Ok(())
}

fn step_name(call: &ProcessorCall) -> String {
    call.classpath
        .first()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) fn step_failed(step: &str, detail: impl Display) -> LauncherError {
    LauncherError::loader(
        ErrorCode::LoaderProcessorFailed,
        "Setting up the mod loader failed",
    )
    .with_param("step", step)
    .with_fix("Try again. If it keeps happening, copy the details and ask for help.")
    .with_detail(detail)
}

/// Run one processor with `java` (`-cp <jar;classpath> <Main-Class> <args>`), then check its outputs.
pub async fn run_processor(
    java: &Path,
    call: &ProcessorCall,
    separator: &str,
    cwd: &Path,
    timeout: Duration,
) -> Result<()> {
    let step = step_name(call);
    let jar = call.classpath[0].clone();
    let main = tokio::task::spawn_blocking(move || main_class(&jar))
        .await
        .map_err(|e| step_failed(&step, e))??;
    let mut command = tokio::process::Command::new(java);
    command
        .args(java_args(call, &main, separator))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = match tokio::time::timeout(timeout, command.output()).await {
        Err(_) => {
            return Err(step_failed(
                &step,
                format!("still running after {} s", timeout.as_secs()),
            ))
        }
        Ok(Err(e)) => return Err(step_failed(&step, format!("couldn't start Java: {e}"))),
        Ok(Ok(output)) => output,
    };
    if !output.status.success() {
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let lines: Vec<&str> = text.lines().collect();
        let tail = lines[lines.len().saturating_sub(OUTPUT_TAIL)..].join("\n");
        return Err(step_failed(
            &step,
            format!("exit code {:?}\n{tail}", output.status.code()),
        ));
    }
    verify_outputs(call).await
}
```

Add `pub mod forge;` at the top of `crates/launcher-core/src/install/mod.rs`, next to `pub mod natives;`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --lib install::forge`
Expected: all PASS. On this PC, `a_failing_processor_reports_its_output` runs the real Java 25.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/launcher-core/src/install
git commit -m "feat(core): plan and run Forge/NeoForge client processors with output checks" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Installer integration: resolve, install, game-jar name, skip marker

**Files:**
- Modify: `crates/launcher-core/src/install/mod.rs` (`ForgeInstall`, `ResolvedVersion::forge`, `ResolvedVersion::client_jar`, `classpath`, `Installer::resolve`, `Installer::install`, `published_sha1` → pub(crate))
- Modify: `crates/launcher-core/src/install/forge.rs` (`fetch_installer`, `resolve_forge`, `install`, `install_current`, `extract_bundled`, `copy_client_jar`)
- Modify: `crates/launcher-core/src/test_support.rs` (`forge_resolved`)
- Modify: every `ResolvedVersion {` literal. Find them with `grep -rn "ResolvedVersion {" --include=*.rs crates src-tauri`; today they are in `install/mod.rs`, `launch/mod.rs` and `play.rs` tests. Add `forge: None,` to each.
- Create: `crates/launcher-core/tests/forge.rs`

**Interfaces:**
- Consumes:
  - Task 4: `meta::forge::{installer_coord, installer_url_at, MavenRepos, OFFICIAL_REPOS, read_installer, read_entry, invalid}`.
  - Task 5: `install::forge::{resolve_data, plan_processors, run_processor, outputs_current, step_failed, PROCESSOR_TIMEOUT}`.
  - Task 3: `java::{ensure_java, console_java}`.
  - `loaders::{check_versions, map_profile_error}`.
- Produces:
  - `install::ForgeInstall { kind: LoaderKind, loader_version: String, installer: PathBuf, installer_sha1: String, profile: InstallProfile }`.
  - `ResolvedVersion { vanilla_id, json, forge: Option<ForgeInstall> }` with `client_jar(&self, &LauncherDirs) -> PathBuf`.
  - `install::forge::fetch_installer(meta, downloader, dirs, kind, mc, loader, MavenRepos) -> Result<(PathBuf, String)>`.
  - `install::forge::resolve_forge(vanilla, kind, mc, loader, installer, sha1) -> Result<(VersionJson, ForgeInstall)>`.
  - Progress task key `install_loader` with params `loader` and `version`.

- [ ] **Step 1: Write the failing unit tests**

Add to `crates/launcher-core/src/test_support.rs`:

```rust
/// Vanilla 1.20.1 merged with Forge 47.4.22, as `Installer::resolve` returns it.
pub fn forge_resolved() -> crate::install::ResolvedVersion {
    crate::install::ResolvedVersion {
        vanilla_id: "1.20.1".into(),
        json: crate::meta::merge::merge(
            version("1.20.1.json"),
            version("forge-1.20.1-47.4.22-version.json"),
        ),
        forge: Some(crate::install::ForgeInstall {
            kind: crate::meta::loaders::LoaderKind::Forge,
            loader_version: "47.4.22".into(),
            installer: "/pl/installer.jar".into(),
            installer_sha1: "afef69499ecb9c712e738d38aa679bd9ed1f3468".into(),
            profile: crate::meta::forge::parse_profile(
                fixture("forge-1.20.1-47.4.22-install_profile.json").as_bytes(),
            )
            .unwrap(),
        }),
    }
}
```

Append inside `mod tests` in `crates/launcher-core/src/install/mod.rs`:

```rust
    #[test]
    fn forge_classpath_names_the_game_jar_after_the_forge_version() {
        let resolved = crate::test_support::forge_resolved();
        let cp = classpath(&resolved, &dirs(), &RuleContext::new(WINDOWS_X64)).unwrap();
        assert_eq!(cp.last().unwrap(), &dirs().client_jar("1.20.1-forge-47.4.22"));
        assert_eq!(cp.len(), 29 + 50 + 1);
        assert_eq!(resolved.client_jar(&dirs()), dirs().client_jar("1.20.1-forge-47.4.22"));
        let vanilla = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json: version("1.20.1.json"),
            forge: None,
        };
        assert_eq!(vanilla.client_jar(&dirs()), dirs().client_jar("1.20.1"));
    }
```

Append inside `mod tests` in `crates/launcher-core/src/install/forge.rs`:

```rust
    #[tokio::test]
    async fn current_install_skips_processors() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join(".perland-loader");
        let made = tmp.path().join("forge-client.jar");
        let call = ProcessorCall {
            classpath: vec![],
            args: vec![],
            outputs: vec![],
            files: vec![made.clone()],
        };
        let local = vec![crate::download::DownloadRequest::new(
            "",
            tmp.path().join("bundled.jar"),
            Hashes::default(),
            None,
        )];
        let calls = std::slice::from_ref(&call);
        assert!(!install_current(&marker, "abc", calls, &local).await, "no marker yet");
        std::fs::write(&marker, "abc").unwrap();
        assert!(!install_current(&marker, "abc", calls, &local).await, "files missing");
        std::fs::write(&made, "x").unwrap();
        std::fs::write(&local[0].dest, "x").unwrap();
        assert!(install_current(&marker, "abc", calls, &local).await);
        assert!(!install_current(&marker, "def", calls, &local).await, "another installer");
        std::fs::remove_file(&made).unwrap();
        assert!(!install_current(&marker, "abc", calls, &local).await, "a file was deleted");
    }

    #[tokio::test]
    async fn bundled_jars_are_extracted_and_verified() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("installer.jar");
        let rel = "net/minecraftforge/forge/1.12.2-x/forge-1.12.2-x.jar";
        let entry = format!("maven/{rel}");
        write_zip(&jar, &[(entry.as_str(), b"hello")]);
        let libs = tmp.path().join("libraries");
        let good = crate::download::DownloadRequest::new("", libs.join(rel), Hashes::sha1(HELLO_SHA1), None);
        let absent = crate::download::DownloadRequest::new("", libs.join("a/b/c.jar"), Hashes::default(), None);
        extract_bundled(&jar, &libs, &[good.clone(), absent.clone()]).unwrap();
        assert_eq!(std::fs::read(&good.dest).unwrap(), b"hello");
        assert!(!absent.dest.exists(), "not bundled: a processor builds it");
        std::fs::remove_file(&good.dest).unwrap();
        let bad = crate::download::DownloadRequest::new("", libs.join(rel), Hashes::sha1("0".repeat(40)), None);
        assert_eq!(
            extract_bundled(&jar, &libs, &[bad]).unwrap_err().code,
            ErrorCode::LoaderInstallerInvalid
        );
    }

    #[tokio::test]
    async fn client_jar_copy_is_made_once() {
        let tmp = tempfile::tempdir().unwrap();
        let vanilla = tmp.path().join("versions/1.20.1/1.20.1.jar");
        let forge = tmp.path().join("versions/1.20.1-forge-47.4.22/1.20.1-forge-47.4.22.jar");
        std::fs::create_dir_all(vanilla.parent().unwrap()).unwrap();
        std::fs::write(&vanilla, "game").unwrap();
        copy_client_jar(&vanilla, &forge).await.unwrap();
        assert_eq!(std::fs::read(&forge).unwrap(), b"game");
        std::fs::write(&forge, "xxxx").unwrap(); // same size: left alone
        copy_client_jar(&vanilla, &forge).await.unwrap();
        assert_eq!(std::fs::read(&forge).unwrap(), b"xxxx");
        std::fs::write(&forge, "x").unwrap(); // different size: replaced
        copy_client_jar(&vanilla, &forge).await.unwrap();
        assert_eq!(std::fs::read(&forge).unwrap(), b"game");
    }
```

- [ ] **Step 2: Write the failing integration tests** (new file `crates/launcher-core/tests/forge.rs`)

```rust
//! Getting a Forge/NeoForge installer: download, checksum, read, merge. Served by wiremock.
use std::{io::Write, path::Path, time::Duration};

use launcher_core::{
    download::{hash::sha1_hex, Downloader},
    install::forge::{fetch_installer, resolve_forge},
    meta::{forge::MavenRepos, loaders::LoaderKind, mojang::VersionJson, MetaClient},
    paths::LauncherDirs,
    ErrorCode,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const FORGE_PROFILE: &str = include_str!("fixtures/forge-1.20.1-47.4.22-install_profile.json");
const FORGE_VERSION: &str = include_str!("fixtures/forge-1.20.1-47.4.22-version.json");
const NEO_PROFILE: &str = include_str!("fixtures/neoforge-21.1.257-install_profile.json");
const NEO_VERSION: &str = include_str!("fixtures/neoforge-21.1.257-version.json");
const FORGE_JAR: &str = "/net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-installer.jar";

fn installer_zip(profile: &str, version: &str) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in [("install_profile.json", profile), ("version.json", version)] {
        w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    }
    w.finish().unwrap().into_inner()
}

async fn serve_installer(server: &MockServer, at: &str, jar: &[u8]) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(jar.to_vec()))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{at}.sha1")))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha1_hex(jar)))
        .mount(server)
        .await;
}

struct Env {
    _tmp: tempfile::TempDir,
    dirs: LauncherDirs,
    meta: MetaClient,
    downloader: Downloader,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = LauncherDirs::new(tmp.path());
    let http = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();
    Env {
        meta: MetaClient::new(http.clone(), dirs.meta()),
        downloader: Downloader::new(http).with_backoff(Duration::from_millis(1)),
        dirs,
        _tmp: tmp,
    }
}

fn repos(uri: &str) -> MavenRepos<'_> {
    MavenRepos { forge: uri, neoforge: uri }
}

fn vanilla() -> VersionJson {
    serde_json::from_str(include_str!("fixtures/1.20.1.json")).unwrap()
}

#[tokio::test]
async fn installer_is_downloaded_verified_and_merged() {
    let server = MockServer::start().await;
    let jar = installer_zip(FORGE_PROFILE, FORGE_VERSION);
    serve_installer(&server, FORGE_JAR, &jar).await;
    let e = env();
    let uri = server.uri();
    let (path, sha1) = fetch_installer(&e.meta, &e.downloader, &e.dirs, LoaderKind::Forge, "1.20.1", "47.4.22", repos(&uri))
        .await
        .unwrap();
    assert_eq!(path, e.dirs.libraries().join(&FORGE_JAR[1..]));
    assert_eq!(sha1, sha1_hex(&jar));
    let (json, install) = resolve_forge(vanilla(), LoaderKind::Forge, "1.20.1", "47.4.22", path, sha1)
        .await
        .unwrap();
    assert_eq!(json.id, "1.20.1-forge-47.4.22");
    assert!(json.inherits_from.is_none());
    assert_eq!(install.loader_version, "47.4.22");
    assert_eq!(install.profile.processors.len(), 10);
}

#[tokio::test]
async fn unknown_loader_version_is_a_loader_error() {
    let server = MockServer::start().await; // answers 404 to everything
    let e = env();
    let uri = server.uri();
    let err = fetch_installer(&e.meta, &e.downloader, &e.dirs, LoaderKind::Forge, "1.20.1", "99.9.9", repos(&uri))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::LoaderVersionNotFound);
    let invalid = fetch_installer(&e.meta, &e.downloader, &e.dirs, LoaderKind::Forge, "1.20.1", "../x", repos(&uri))
        .await
        .unwrap_err();
    assert_eq!(invalid.code, ErrorCode::VersionInvalid);
}

#[tokio::test]
async fn installer_for_another_minecraft_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let jar = tmp.path().join("neoforge-21.1.257-installer.jar");
    std::fs::write(&jar, installer_zip(NEO_PROFILE, NEO_VERSION)).unwrap();
    // 21.1.x is for 1.21.1; this instance is 1.21.
    let err = resolve_forge(vanilla(), LoaderKind::NeoForge, "1.21", "21.1.257", jar, "s".into())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::LoaderVersionNotFound);
    assert_eq!(err.params["version"], "1.21");
}

#[tokio::test]
async fn installer_resolves_offline_after_the_first_download() {
    let e = env();
    let jar = installer_zip(FORGE_PROFILE, FORGE_VERSION);
    let uri = {
        let server = MockServer::start().await;
        serve_installer(&server, FORGE_JAR, &jar).await;
        let uri = server.uri();
        fetch_installer(&e.meta, &e.downloader, &e.dirs, LoaderKind::Forge, "1.20.1", "47.4.22", repos(&uri))
            .await
            .unwrap();
        uri
    }; // the server is gone now
    let (path, sha1) = fetch_installer(&e.meta, &e.downloader, &e.dirs, LoaderKind::Forge, "1.20.1", "47.4.22", repos(&uri))
        .await
        .unwrap();
    assert!(Path::new(&path).is_file());
    assert_eq!(sha1, sha1_hex(&jar));
}
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -p launcher-core --test forge` and `cargo test -p launcher-core --lib install::`
Expected: compile errors: `fetch_installer`, `resolve_forge`, `ForgeInstall` and the `forge` field are not found.

- [ ] **Step 4: Implement the types and classpath** (`crates/launcher-core/src/install/mod.rs`)

```rust
/// What a Forge or NeoForge version adds on top of its merged JSON: the installer and its profile.
#[derive(Debug, Clone)]
pub struct ForgeInstall {
    pub kind: LoaderKind,
    pub loader_version: String,
    /// The installer jar, kept in the libraries folder (its maven path).
    pub installer: PathBuf,
    pub installer_sha1: String,
    pub profile: InstallProfile,
}

/// The version to launch: `json` is vanilla, or vanilla merged with a loader profile.
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    pub vanilla_id: String,
    pub json: VersionJson,
    /// Forge and NeoForge: the installer steps `install` also runs. `None` for vanilla, Fabric, Quilt.
    pub forge: Option<ForgeInstall>,
}

impl ResolvedVersion {
    /// The game jar on the classpath. Forge and NeoForge tell their launcher to ignore
    /// `${version_name}.jar`, so their copy is named after the merged id, the way the official
    /// launcher lays it out; everyone else uses the vanilla jar where it was downloaded.
    pub fn client_jar(&self, dirs: &LauncherDirs) -> PathBuf {
        match self.forge {
            Some(_) => dirs.client_jar(&self.json.id),
            None => dirs.client_jar(&self.vanilla_id),
        }
    }
}
```

Imports to add: `crate::meta::forge::{InstallProfile, OFFICIAL_REPOS}`. In `classpath`, replace `cp.push(dirs.client_jar(&resolved.vanilla_id));` with `cp.push(resolved.client_jar(dirs));`. Make `published_sha1` and `join_remote` `pub(crate)`. Add `forge: None,` to every existing `ResolvedVersion {` literal (the `grep` above).

- [ ] **Step 5: Implement resolve and install** (`crates/launcher-core/src/install/forge.rs`, below the processor code)

```rust
/// Marks a finished loader install; holds the installer's sha1.
const MARKER: &str = ".perland-loader";
/// Bundled jars and data files inside an installer stay well under this.
const MAX_BUNDLED_BYTES: u64 = 256 * 1024 * 1024;

/// Download the installer jar into the libraries folder, verified against the sha1 maven
/// publishes next to it. Both go through caches, so this works offline after the first time.
pub async fn fetch_installer(
    meta: &MetaClient,
    downloader: &Downloader,
    dirs: &LauncherDirs,
    kind: LoaderKind,
    mc: &str,
    loader: &str,
    repos: MavenRepos<'_>,
) -> Result<(PathBuf, String)> {
    loaders::check_versions(mc, loader)?;
    let (Some(coord), Some(url)) = (
        installer_coord(kind, mc, loader),
        installer_url_at(kind, mc, loader, repos),
    ) else {
        return Err(LauncherError::loader(
            ErrorCode::LoaderUnsupported,
            format!("{kind:?} has no installer"),
        )
        .with_param("loader", format!("{kind:?}")));
    };
    let dest = join_remote(&dirs.libraries(), &coord.path())?;
    let sha1 = published_sha1(meta, &url)
        .await
        .map_err(|e| loaders::map_profile_error(kind, mc, loader, e))?;
    downloader
        .fetch(&DownloadRequest::new(url, dest.clone(), Hashes::sha1(&sha1), None))
        .await?;
    Ok((dest, sha1))
}

/// Read the installer and merge its version JSON onto vanilla.
pub async fn resolve_forge(
    vanilla: VersionJson,
    kind: LoaderKind,
    mc: &str,
    loader: &str,
    installer: PathBuf,
    installer_sha1: String,
) -> Result<(VersionJson, ForgeInstall)> {
    let jar = installer.clone();
    let contents = tokio::task::spawn_blocking(move || read_installer(&jar))
        .await
        .map_err(invalid)??;
    let for_mc = contents
        .version
        .inherits_from
        .clone()
        .unwrap_or_else(|| contents.profile.minecraft.clone());
    if for_mc != mc {
        return Err(LauncherError::loader(
            ErrorCode::LoaderVersionNotFound,
            format!("{kind:?} {loader} is for Minecraft {for_mc}, not {mc}"),
        )
        .with_param("loader", format!("{kind:?}"))
        .with_param("loader_version", loader)
        .with_param("version", mc)
        .with_fix("Pick a loader version from the list"));
    }
    Ok((
        merge(vanilla, contents.version),
        ForgeInstall {
            kind,
            loader_version: loader.to_string(),
            installer,
            installer_sha1,
            profile: contents.profile,
        },
    ))
}

/// A previous install with this exact installer finished, and every file it touched is still there.
async fn install_current(
    marker: &Path,
    installer_sha1: &str,
    calls: &[ProcessorCall],
    local: &[DownloadRequest],
) -> bool {
    let finished = tokio::fs::read_to_string(marker)
        .await
        .is_ok_and(|s| s.trim() == installer_sha1);
    finished
        && calls
            .iter()
            .flat_map(|c| &c.files)
            .chain(local.iter().map(|d| &d.dest))
            .all(|p| p.is_file())
}

/// Older installers bundle jars under `maven/<path>`: copy each wanted one out, verified. Jars
/// the installer doesn't bundle are left for the processors to build. Blocking.
fn extract_bundled(installer: &Path, libraries: &Path, wanted: &[DownloadRequest]) -> Result<()> {
    if wanted.is_empty() {
        return Ok(());
    }
    let mut archive = zip::ZipArchive::new(std::fs::File::open(installer)?).map_err(invalid)?;
    for req in wanted {
        if req.dest.is_file() && req.hashes.verify_file(&req.dest).unwrap_or(false) {
            continue;
        }
        let Ok(rel) = req.dest.strip_prefix(libraries) else {
            continue;
        };
        let rel: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let entry = format!("maven/{}", rel.join("/"));
        if archive.index_for_name(&entry).is_none() {
            continue;
        }
        let bytes = read_entry(&mut archive, &entry, MAX_BUNDLED_BYTES)?;
        if !req.hashes.verify(&bytes) {
            return Err(invalid(format!("{entry} doesn't match its checksum")));
        }
        if let Some(parent) = req.dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = req.dest.with_extension("jar.tmp");
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &req.dest)?;
    }
    Ok(())
}

/// Extract the installer files the data values name (e.g. `data/client.lzma`). Blocking.
fn extract_files(installer: &Path, files: &[(String, PathBuf)]) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(installer)?).map_err(invalid)?;
    for (entry, dest) in files {
        let bytes = read_entry(&mut archive, entry, MAX_BUNDLED_BYTES)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(dest, bytes)?;
    }
    Ok(())
}

/// Copy the vanilla jar to `dest` unless a same-size copy is already there.
async fn copy_client_jar(vanilla: &Path, dest: &Path) -> Result<()> {
    if vanilla == dest {
        return Ok(());
    }
    let want = tokio::fs::metadata(vanilla).await?.len();
    if tokio::fs::metadata(dest).await.is_ok_and(|m| m.len() == want) {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    tokio::fs::copy(vanilla, &tmp).await?;
    tokio::fs::rename(&tmp, dest).await?;
    Ok(())
}

/// Every locally provided library must exist (and match its sha1, when one is published).
async fn check_local(local: &[DownloadRequest]) -> Result<()> {
    for req in local {
        let (path, hashes) = (req.dest.clone(), req.hashes.clone());
        let ok = tokio::task::spawn_blocking(move || {
            path.is_file() && hashes.verify_file(&path).unwrap_or(false)
        })
        .await
        .unwrap_or(false);
        if !ok {
            return Err(step_failed(
                "",
                format!("{} wasn't built", req.dest.display()),
            ));
        }
    }
    Ok(())
}

/// The loader half of `Installer::install`: profile libraries, bundled jars, processors (unless a
/// matching install already finished), then the game jar under the loader's version name.
pub(crate) async fn install(
    inst: &Installer,
    resolved: &ResolvedVersion,
    forge: &ForgeInstall,
    version_local: &[DownloadRequest],
    sink: &EventSink,
) -> Result<()> {
    let dirs = &inst.dirs;
    let libraries = dirs.libraries();
    let task = TaskId::new("install_loader")
        .with("loader", format!("{:?}", forge.kind))
        .with("version", &forge.loader_version);
    let ctx = RuleContext::new(inst.platform);
    let mut local = version_local.to_vec();
    let mut downloads = Vec::new();
    for lib in forge.profile.libraries.iter().filter(|l| library_applies(l, &ctx)) {
        if let Some(d) = library_artifact(lib, dirs)? {
            if is_local(&d) {
                local.push(d);
            } else {
                downloads.push(d);
            }
        }
    }
    let (installer, libs, wanted) = (forge.installer.clone(), libraries.clone(), local.clone());
    tokio::task::spawn_blocking(move || extract_bundled(&installer, &libs, &wanted))
        .await
        .map_err(invalid)??;
    fill_missing_sha1(&inst.meta, &mut downloads).await?;
    inst.downloader.fetch_all(downloads, &task, sink).await?;

    let version_dir = dirs.version_dir(&resolved.json.id);
    let extract_dir = version_dir.join(".installer");
    let vanilla_jar = dirs.client_jar(&resolved.vanilla_id);
    let data = resolve_data(
        &forge.profile,
        &DataContext {
            libraries: &libraries,
            installer: &forge.installer,
            extract_dir: &extract_dir,
            minecraft_jar: &vanilla_jar,
            minecraft: &resolved.vanilla_id,
            root: dirs.root(),
        },
    )?;
    let calls = plan_processors(&forge.profile, &data, &libraries)?;
    let marker = version_dir.join(MARKER);
    if !install_current(&marker, &forge.installer_sha1, &calls, &local).await {
        let java = java::console_java(
            &java::ensure_java(&inst.meta, &inst.downloader, &dirs.java(), inst.platform, &resolved.json, sink)
                .await?,
        );
        tokio::fs::create_dir_all(&extract_dir).await?;
        let (installer, extract) = (forge.installer.clone(), data.extract.clone());
        tokio::task::spawn_blocking(move || extract_files(&installer, &extract))
            .await
            .map_err(invalid)??;
        let total = calls.len() as u64;
        for (i, call) in calls.iter().enumerate() {
            sink(Event::Progress { task: task.clone(), done: i as u64, total, bytes: 0 });
            if !outputs_current(call).await {
                run_processor(
                    &java,
                    call,
                    inst.platform.classpath_separator(),
                    &extract_dir,
                    PROCESSOR_TIMEOUT,
                )
                .await?;
            }
        }
        sink(Event::Progress { task: task.clone(), done: total, total, bytes: 0 });
        let _ = tokio::fs::remove_dir_all(&extract_dir).await;
        check_local(&local).await?;
    }
    copy_client_jar(&vanilla_jar, &resolved.client_jar(dirs)).await?;
    write_atomic(&marker, forge.installer_sha1.as_bytes()).await?;
    Ok(())
}
```

Add the imports this needs at the top of `install/forge.rs`:

```rust
use super::{
    fill_missing_sha1, is_local, join_remote, library_artifact, published_sha1, ForgeInstall,
    Installer, ResolvedVersion,
};
use crate::{
    download::{DownloadRequest, Downloader},
    events::{Event, EventSink, TaskId},
    fsutil::write_atomic,
    java,
    meta::{
        forge::{installer_coord, installer_url_at, read_installer, MavenRepos},
        loaders::{self, LoaderKind},
        merge::merge,
        mojang::VersionJson,
        rules::{library_applies, RuleContext},
        MetaClient,
    },
    paths::LauncherDirs,
};
```

`step_failed` takes the step name as `&str` (Task 5). `check_local` passes `""` because no single processor is to blame.

In `Installer::resolve`, replace the `let json = match loader { … };` block with:

```rust
        let (json, forge) = match loader {
            Some((kind @ (LoaderKind::Forge | LoaderKind::NeoForge), version)) => {
                let (installer, sha1) = forge::fetch_installer(
                    &self.meta,
                    &self.downloader,
                    &self.dirs,
                    kind,
                    mc,
                    version,
                    OFFICIAL_REPOS,
                )
                .await?;
                let (json, install) =
                    forge::resolve_forge(vanilla.clone(), kind, mc, version, installer, sha1)
                        .await?;
                (json, Some(install))
            }
            Some((kind, version)) => (
                merge(
                    vanilla.clone(),
                    loaders::fetch_profile(&self.meta, kind, mc, version).await?,
                ),
                None,
            ),
            None => (vanilla.clone(), None),
        };
        check_version_id(&json.id)?;
        Ok(ResolvedVersion {
            vanilla_id: vanilla.id,
            json,
            forge,
        })
```

In `Installer::install`, right after the big `fetch_all(...)` and before natives extraction:

```rust
        if let Some(install) = &resolved.forge {
            forge::install(self, resolved, install, &libraries.local, sink).await?;
        }
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p launcher-core`
Expected: all PASS, including the new `tests/forge.rs` and the updated `play`/`launch` tests.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/launcher-core
git commit -m "feat(core): install Forge and NeoForge through their installers" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Long command lines go through a Java argfile

Windows refuses a command line over 32,767 characters, and Forge's classpath under a deep data folder can get close. Java 9+ reads arguments from `@file`.

**Files:**
- Modify: `crates/launcher-core/src/launch/mod.rs` (`ARGFILE_THRESHOLD`, `command_line_len`, `wants_argfile`, `argfile_text`, `with_argfile`)
- Modify: `crates/launcher-core/src/play.rs` (`start` uses them; `java_major`)

**Interfaces:**
- Consumes: `fsutil::write_atomic`, `java::{probe, required_major}`, `instances::InstanceStore::perland_dir`.
- Produces:
  - `launch::ARGFILE_THRESHOLD: usize = 30_000`.
  - `launch::command_line_len(&LaunchCommand) -> usize`.
  - `launch::wants_argfile(&LaunchCommand, Option<u32>) -> bool`.
  - `launch::argfile_text(&[String]) -> String`.
  - `launch::with_argfile(&LaunchCommand, main_class: &str, path: &Path, secret: &str) -> Result<LaunchCommand>` (async).
  - The `LaunchCommand`, `build_command` and `spawn` signatures are unchanged.

- [ ] **Step 1: Write the failing tests** (append inside `mod tests` in `crates/launcher-core/src/launch/mod.rs`)

```rust
    #[test]
    fn forge_ignore_list_names_the_jar_on_the_classpath() {
        let cmd = build(&crate::test_support::forge_resolved(), WINDOWS_X64);
        let jar = cmd
            .classpath
            .last()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let ignore = cmd
            .args
            .iter()
            .find(|a| a.starts_with("-DignoreList="))
            .unwrap();
        assert!(ignore.ends_with(&format!(",{jar}")), "{ignore} must name {jar}");
        assert_eq!(arg_after(&cmd.args, "--launchTarget"), "forgeclient");
        assert_eq!(arg_after(&cmd.args, "--username"), "Steve");
        assert!(!cmd.args.iter().any(|x| x.contains("${")), "{:?}", cmd.args);
    }

    fn long_command() -> LaunchCommand {
        LaunchCommand {
            program: "/j/bin/java".into(),
            args: vec![
                "-Xmx1M".into(),
                "-cp".into(),
                "x".repeat(ARGFILE_THRESHOLD),
                "net.minecraft.client.main.Main".into(),
                "--accessToken".into(),
                "tok-0123456789".into(),
            ],
            cwd: "/pl/instances/x/minecraft".into(),
            classpath: vec![],
        }
    }

    #[test]
    fn argfile_only_for_long_lines_on_java_9_and_newer() {
        let long = long_command();
        assert!(command_line_len(&long) > ARGFILE_THRESHOLD);
        assert!(wants_argfile(&long, Some(17)));
        assert!(!wants_argfile(&long, Some(8)));
        assert!(!wants_argfile(&long, None));
        let mut short = long;
        short.args[2] = "a.jar".into();
        assert!(!wants_argfile(&short, Some(17)));
    }

    #[test]
    fn argfile_text_quotes_every_argument() {
        let text = argfile_text(&[
            r#"C:\Program Files\x y#z"q"#.to_string(),
            "plain".to_string(),
        ]);
        assert_eq!(text, "\"C:\\\\Program Files\\\\x y#z\\\"q\"\n\"plain\"\n");
    }

    #[tokio::test]
    async fn argfile_keeps_game_arguments_on_the_command_line() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(".perland/jvm-args.txt");
        let cmd = long_command();
        let out = with_argfile(&cmd, "net.minecraft.client.main.Main", &file, "tok-0123456789")
            .await
            .unwrap();
        assert_eq!(
            out.args,
            vec![
                format!("@{}", file.display()),
                "net.minecraft.client.main.Main".to_string(),
                "--accessToken".to_string(),
                "tok-0123456789".to_string(),
            ]
        );
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.starts_with("\"-Xmx1M\"\n\"-cp\"\n"));
        assert!(!written.contains("tok-"), "the token never touches the disk");
        assert_eq!(out.cwd, cmd.cwd);
    }

    #[tokio::test]
    async fn argfile_is_skipped_when_a_jvm_argument_holds_the_secret() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("jvm-args.txt");
        let mut cmd = long_command();
        cmd.args[0] = "-Dleak=tok-0123456789".into();
        let out = with_argfile(&cmd, "net.minecraft.client.main.Main", &file, "tok-0123456789")
            .await
            .unwrap();
        assert_eq!(out.args, cmd.args);
        assert!(!file.exists());
    }

    #[test]
    fn java_reads_the_argfile_back() {
        // Skipped where no java is installed.
        if std::process::Command::new("java").arg("-version").output().is_err() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("args.txt");
        let tricky = r#"C:\Program Files\x y#z"q"#;
        std::fs::write(
            &file,
            argfile_text(&[format!("-Dperland.tricky={tricky}"), "-Dperland.plain=ok".into()]),
        )
        .unwrap();
        let out = std::process::Command::new("java")
            .arg(format!("@{}", file.display()))
            .args(["-XshowSettings:properties", "-version"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(text.contains(&format!("perland.tricky = {tricky}")), "{text}");
        assert!(text.contains("perland.plain = ok"), "{text}");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib launch::`
Expected: compile errors: `ARGFILE_THRESHOLD`, `command_line_len`, `wants_argfile`, `argfile_text` and `with_argfile` are not found. `forge_ignore_list_names_the_jar_on_the_classpath` compiles against Task 6 and already passes. It is the Review Focus 2 pin.

- [ ] **Step 3: Implement** (in `crates/launcher-core/src/launch/mod.rs`, after `missing_files`)

```rust
/// Longest command line started directly. Windows refuses anything over 32,767 characters; past
/// this, the JVM arguments move into a Java argument file.
pub const ARGFILE_THRESHOLD: usize = 30_000;

/// About how long the OS command line is: the program and every argument, quoted and spaced.
pub fn command_line_len(cmd: &LaunchCommand) -> usize {
    cmd.program.as_os_str().len() + 2 + cmd.args.iter().map(|a| a.len() + 3).sum::<usize>()
}

/// Java reads `@argfile`s from version 9 on; an unknown version gets the plain command line.
pub fn wants_argfile(cmd: &LaunchCommand, java_major: Option<u32>) -> bool {
    command_line_len(cmd) > ARGFILE_THRESHOLD && java_major.is_some_and(|m| m >= 9)
}

/// A Java `@argfile` body: one argument per line, each in double quotes, with `\` and `"`
/// escaped (inside quotes Java treats `\` as an escape; outside them `#` starts a comment).
pub fn argfile_text(args: &[String]) -> String {
    let mut out = String::new();
    for arg in args {
        out.push('"');
        for c in arg.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                _ => out.push(c),
            }
        }
        out.push_str("\"\n");
    }
    out
}

/// Move the JVM arguments (everything before `main_class`) into the argfile at `path`. The game
/// arguments, which carry the access token, stay on the command line. If any JVM argument holds
/// `secret`, nothing is written and the command comes back unchanged.
pub async fn with_argfile(
    cmd: &LaunchCommand,
    main_class: &str,
    path: &Path,
    secret: &str,
) -> Result<LaunchCommand> {
    let Some(split) = cmd.args.iter().position(|a| a == main_class) else {
        return Ok(cmd.clone());
    };
    let jvm = &cmd.args[..split];
    if secret.len() >= 4 && jvm.iter().any(|a| a.contains(secret)) {
        return Ok(cmd.clone());
    }
    write_atomic(path, argfile_text(jvm).as_bytes()).await?;
    let mut args = vec![format!("@{}", path.display())];
    args.extend_from_slice(&cmd.args[split..]);
    Ok(LaunchCommand {
        args,
        ..cmd.clone()
    })
}
```

Add `fsutil::write_atomic` to the `crate::{…}` imports.

In `crates/launcher-core/src/play.rs`, `start` becomes:

```rust
/// Build the command, check its files, and start the game with a redacted log. A command line
/// too long for the OS puts its JVM arguments in `<instance>/.perland/jvm-args.txt`.
pub async fn start(spec: &StartSpec<'_>, sink: EventSink) -> Result<GameProcess> {
    let mut cmd = command(spec)?;
    check_files(&cmd)?;
    if launch::command_line_len(&cmd) > launch::ARGFILE_THRESHOLD
        && launch::wants_argfile(&cmd, java_major(spec).await)
    {
        let path = InstanceStore::new(spec.dirs.instances())
            .perland_dir(&spec.instance.id)
            .join("jvm-args.txt");
        cmd = launch::with_argfile(
            &cmd,
            &spec.prepared.resolved.json.main_class,
            &path,
            &spec.session.access_token,
        )
        .await?;
    }
    let redactor = Redactor::for_current_user().with_secret(spec.session.access_token.clone());
    let log = spec
        .dirs
        .logs()
        .join(format!("{}-latest.log", spec.instance.id));
    launch::spawn(&cmd, redactor, sink, &log).await
}

/// The Java major version: what the version asks for when the launcher picks Java (Mojang's
/// runtime or an exact-match install), else what the chosen Java reports.
async fn java_major(spec: &StartSpec<'_>) -> Option<u32> {
    match &spec.instance.java.path {
        None => Some(java::required_major(&spec.prepared.resolved.json)),
        Some(path) => java::probe(path).await.ok(),
    }
}
```

The length check comes first, so the custom-Java `probe` (a 10 s budget) only runs when an argfile might be needed. Add `instances::InstanceStore` to `play.rs`'s imports.

Add a play test (inside `mod tests` in `play.rs`, reusing the setup of `start_refuses_when_files_are_missing`):

```rust
    #[tokio::test]
    async fn automatic_java_major_comes_from_the_version() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = LauncherDirs::new(tmp.path());
        let store = InstanceStore::new(dirs.instances());
        let instance = store
            .create("Smoke", GameSpec { minecraft: "1.20.1".into(), loader: None })
            .await
            .unwrap();
        let prepared = Prepared {
            resolved: ResolvedVersion {
                vanilla_id: "1.20.1".into(),
                json: version("1.20.1.json"),
                forge: None,
            },
            java: tmp.path().join("java.exe"),
        };
        let session = offline_account("Steve").unwrap().offline_session();
        let game_dir = store.game_dir(&instance.id);
        let spec = StartSpec {
            dirs: &dirs,
            game_dir: &game_dir,
            instance: &instance,
            prepared: &prepared,
            session: &session,
            platform: Platform::current(),
            launcher_version: "0.1.0",
        };
        assert_eq!(java_major(&spec).await, Some(17));
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --lib launch:: play::`
Expected: all PASS. `java_reads_the_argfile_back` runs the real Java on this PC and on the GitHub runners.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/launcher-core/src/launch/mod.rs crates/launcher-core/src/play.rs
git commit -m "feat(core): put JVM arguments in an argfile when the command line is too long" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Catalog: Forge and NeoForge version lists

**Files:**
- Modify: `crates/launcher-core/src/meta/catalog.rs`
- Modify: `crates/launcher-core/tests/catalog.rs` (replace `forge_is_not_offered_yet`)

**Interfaces:**
- Consumes: `meta::forge::{MavenRepos, OFFICIAL_REPOS}`, `MetaClient::get_bytes`, `compare_versions`.
- Produces:
  - `catalog::forge_versions_at(&MetaClient, LoaderKind, mc: &str, MavenRepos) -> Result<Vec<LoaderVersion>>`.
  - `catalog::neoforge_prefix(&str) -> Option<String>`.
  - `loader_versions` / `loader_versions_at` now answer for Forge and NeoForge, with unchanged signatures.

The list sources are each repository's standard `maven-metadata.xml`:
- **Forge:** `net/minecraftforge/forge`, whose versions are `<mc>-<forge>`.
- **NeoForge 1.20.1:** `net/neoforged/forge` (`1.20.1-<v>`).
- **NeoForge for every other Minecraft version:** `net/neoforged/neoforge`, where `21.1.x` is 1.21.1, `21.0.x` is 1.21 and year-style `26.3.0.x` is 26.3.

Forge versions are all marked stable, because Forge publishes no pre-releases to maven. NeoForge versions with a `-` (`-beta`) are not stable.

- [ ] **Step 1: Write the failing tests**

In `crates/launcher-core/tests/catalog.rs`, replace `forge_is_not_offered_yet` with:

```rust
const FORGE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata><groupId>net.minecraftforge</groupId><artifactId>forge</artifactId>
<versioning><latest>1.21.1-52.1.0</latest><versions>
<version>1.20.1-47.4.9</version><version>1.20.1-47.4.22</version>
<version>1.20.10-99.0.0</version><version>1.21.1-52.1.0</version>
<version>1.20.1-47.4.10</version>
</versions></versioning></metadata>"#;

const NEO_XML: &str = r#"<metadata><versioning><versions>
<version>21.0.167</version><version>21.1.256</version><version>21.1.257</version>
<version>21.1.258-beta</version><version>21.10.5</version><version>26.3.0.64-beta</version>
<version>0.25w14craftmine.3-beta</version>
</versions></versioning></metadata>"#;

const NEO_FORGE_XML: &str = r#"<metadata><versioning><versions>
<version>1.20.1-47.1.105</version><version>1.20.1-47.1.106</version><version>47.1.82</version>
</versions></versioning></metadata>"#;

fn repos(uri: &str) -> MavenRepos<'_> {
    MavenRepos { forge: uri, neoforge: uri }
}

fn versions(list: &[LoaderVersion]) -> Vec<(&str, bool)> {
    list.iter().map(|v| (v.version.as_str(), v.stable)).collect()
}

#[tokio::test]
async fn forge_versions_for_one_minecraft_newest_first() {
    let server = MockServer::start().await;
    serve(&server, "/net/minecraftforge/forge/maven-metadata.xml", 200, FORGE_XML).await;
    let tmp = tempfile::tempdir().unwrap();
    let uri = server.uri();
    let list = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.20.1", repos(&uri))
        .await
        .unwrap();
    assert_eq!(
        versions(&list),
        vec![("47.4.22", true), ("47.4.10", true), ("47.4.9", true)]
    );
    let none = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.19.2", repos(&uri))
        .await
        .unwrap();
    assert!(none.is_empty());
}

#[tokio::test]
async fn neoforge_versions_follow_the_minecraft_version() {
    let server = MockServer::start().await;
    serve(&server, "/net/neoforged/neoforge/maven-metadata.xml", 200, NEO_XML).await;
    let tmp = tempfile::tempdir().unwrap();
    let (uri, m) = (server.uri(), meta(tmp.path()));
    assert_eq!(
        versions(&neo(&m, &uri, "1.21.1").await),
        vec![("21.1.258-beta", false), ("21.1.257", true), ("21.1.256", true)]
    );
    assert_eq!(versions(&neo(&m, &uri, "1.21").await), vec![("21.0.167", true)]);
    assert_eq!(versions(&neo(&m, &uri, "26.3").await), vec![("26.3.0.64-beta", false)]);
    assert!(neo(&m, &uri, "26.4-snapshot-3").await.is_empty());
}

async fn neo(m: &MetaClient, uri: &str, mc: &str) -> Vec<LoaderVersion> {
    forge_versions_at(m, LoaderKind::NeoForge, mc, repos(uri))
        .await
        .unwrap()
}

#[tokio::test]
async fn neoforge_for_1_20_1_comes_from_its_forge_artifact() {
    let server = MockServer::start().await;
    serve(&server, "/net/neoforged/forge/maven-metadata.xml", 200, NEO_FORGE_XML).await;
    let tmp = tempfile::tempdir().unwrap();
    let uri = server.uri();
    let list = forge_versions_at(&meta(tmp.path()), LoaderKind::NeoForge, "1.20.1", repos(&uri))
        .await
        .unwrap();
    assert_eq!(versions(&list), vec![("47.1.106", true), ("47.1.105", true)]);
}

#[tokio::test]
async fn missing_forge_metadata_is_an_error() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let uri = server.uri();
    let err = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.20.1", repos(&uri))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaNotFound);
}
```

Update the imports: `meta::{catalog::{forge_versions_at, loader_versions_at, minecraft_versions_at, LoaderVersion, McKind}, forge::MavenRepos, loaders::LoaderKind, MetaClient}`. Drop `loader_versions_at` if nothing else uses it.

Inside `mod tests` in `crates/launcher-core/src/meta/catalog.rs`:

```rust
    #[test]
    fn neoforge_prefixes() {
        assert_eq!(neoforge_prefix("1.21.1").as_deref(), Some("21.1."));
        assert_eq!(neoforge_prefix("1.21").as_deref(), Some("21.0."));
        assert_eq!(neoforge_prefix("1.20.4").as_deref(), Some("20.4."));
        assert_eq!(neoforge_prefix("26.3").as_deref(), Some("26.3.0."));
        assert_eq!(neoforge_prefix("26.3.1").as_deref(), Some("26.3.1."));
        assert_eq!(neoforge_prefix("26.4-snapshot-3"), None);
        assert_eq!(neoforge_prefix("b1.7.3"), None);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --test catalog` and `cargo test -p launcher-core --lib meta::catalog`
Expected: compile errors: `forge_versions_at` and `neoforge_prefix` are not found.

- [ ] **Step 3: Implement** (in `crates/launcher-core/src/meta/catalog.rs`)

In `loader_versions_at`, replace the `LoaderKind::Forge | LoaderKind::NeoForge => { return Err(…) }` arm with:

```rust
        LoaderKind::Forge | LoaderKind::NeoForge => {
            return forge_versions_at(meta, kind, mc, OFFICIAL_REPOS).await
        }
```

Add, with `use super::forge::{MavenRepos, OFFICIAL_REPOS};` at the top:

```rust
/// Forge or NeoForge versions for one Minecraft version, newest first, from the repository's
/// `maven-metadata.xml`. Empty when there are none for it.
pub async fn forge_versions_at(
    meta: &MetaClient,
    kind: LoaderKind,
    mc: &str,
    repos: MavenRepos<'_>,
) -> Result<Vec<LoaderVersion>> {
    if !valid_component(mc) {
        return Err(LauncherError::loader(
            ErrorCode::VersionInvalid,
            format!("\"{mc}\" isn't a valid version"),
        )
        .with_param("value", mc));
    }
    let forge = repos.forge.trim_end_matches('/');
    let neo = repos.neoforge.trim_end_matches('/');
    // (metadata URL, version prefix, whether the prefix is part of the loader version)
    let (url, prefix, keep_prefix) = match kind {
        LoaderKind::Forge => (
            format!("{forge}/net/minecraftforge/forge/maven-metadata.xml"),
            format!("{mc}-"),
            false,
        ),
        LoaderKind::NeoForge if mc == "1.20.1" => (
            format!("{neo}/net/neoforged/forge/maven-metadata.xml"),
            "1.20.1-".to_string(),
            false,
        ),
        LoaderKind::NeoForge => match neoforge_prefix(mc) {
            Some(prefix) => (
                format!("{neo}/net/neoforged/neoforge/maven-metadata.xml"),
                prefix,
                true,
            ),
            None => return Ok(Vec::new()),
        },
        LoaderKind::Fabric | LoaderKind::Quilt => {
            return Err(LauncherError::loader(
                ErrorCode::LoaderUnsupported,
                format!("{kind:?} isn't listed on maven"),
            )
            .with_param("loader", format!("{kind:?}")))
        }
    };
    let bytes = meta.get_bytes(&url).await?;
    let mut out: Vec<LoaderVersion> = maven_versions(&String::from_utf8_lossy(&bytes))
        .into_iter()
        .filter(|v| v.starts_with(&prefix))
        .map(|v| if keep_prefix { v } else { v[prefix.len()..].to_string() })
        .filter(|v| valid_component(v))
        .map(|version| LoaderVersion {
            stable: kind == LoaderKind::Forge || !version.contains('-'),
            version,
        })
        .collect();
    out.sort_by(|a, b| compare_versions(&b.version, &a.version));
    out.dedup_by(|a, b| a.version == b.version);
    Ok(out)
}

/// NeoForge numbers releases after Minecraft: `21.1.x` is 1.21.1, `21.0.x` is 1.21, and
/// year-style `26.3.0.x` is 26.3. `None` for ids NeoForge can't be for (snapshots, old betas).
pub fn neoforge_prefix(mc: &str) -> Option<String> {
    let parts: Vec<&str> = mc.split('.').collect();
    if !parts
        .iter()
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    match parts.as_slice() {
        ["1", minor] => Some(format!("{minor}.0.")),
        ["1", minor, patch] => Some(format!("{minor}.{patch}.")),
        [year, minor] => Some(format!("{year}.{minor}.0.")),
        [year, minor, patch] => Some(format!("{year}.{minor}.{patch}.")),
        _ => None,
    }
}

/// Every `<version>` in a maven-metadata.xml, in file order.
fn maven_versions(xml: &str) -> Vec<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|s| s.split_once("</version>"))
        .map(|(v, _)| v.trim().to_string())
        .collect()
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --test catalog --lib meta::catalog`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/launcher-core/src/meta/catalog.rs crates/launcher-core/tests/catalog.rs
git commit -m "feat(core): Forge and NeoForge version lists for the New-instance picker" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: The desktop app offers Forge and NeoForge

The look stays the same. This adds two options to the existing loader control and removes the "can't change a Forge instance" lock. It uses the existing `.seg` style. If the five segments wrap badly at the narrowest window width, stop and route the layout through `/impeccable` (CLAUDE.md) rather than restyling by hand.

**Files:**
- Modify: `crates/launcher-app/src/app.rs` (`instance_create` stops refusing Forge; update test `create_uses_the_default_memory_and_refuses_forge`)
- Modify: `ui/src/features/new-instance/useVersionChoice.ts`, `ui/src/features/new-instance/VersionFields.tsx`
- Modify: `ui/src/features/instance-settings/InstanceSettings.tsx`
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Test: `ui/src/features/new-instance/useVersionChoice.test.tsx`, `ui/src/features/new-instance/NewInstance.test.tsx`, `ui/src/features/instance-settings/InstanceSettings.test.tsx`, `ui/src/features/mods/Mods.test.tsx`

**Interfaces:**
- Consumes: the `catalog_loaders` command, which now serves Forge/NeoForge through `loader_versions` (Task 8).
- Produces:
  - `LoaderChoice = 'none' | LoaderKind`.
  - The `newInstance.loaderForge` and `newInstance.loaderNeoForge` strings.

- [ ] **Step 1: Write the failing tests**

`crates/launcher-app/src/app.rs`: rename `create_uses_the_default_memory_and_refuses_forge` to `create_uses_the_default_memory_and_accepts_forge`, and change its tail to:

```rust
        let created_forge = core.instance_create(forge).await.unwrap();
        assert_eq!(created_forge.game.loader.unwrap().kind, LoaderKind::Forge);
        assert_eq!(core.instances_list().await.unwrap().instances.len(), 2);
```

`ui/src/features/new-instance/useVersionChoice.test.tsx`: replace the first test with:

```tsx
it('keeps a Forge instance on Forge with its version', async () => {
  renderApp(<Probe initial={{ minecraft: '1.20.1', loader: { kind: 'forge', version: '47.2.0' } }} />);
  await waitFor(() =>
    expect(JSON.parse(screen.getByTestId('probe').textContent ?? '')).toEqual({
      minecraft: '1.20.1',
      loader: 'forge',
      chosenLoader: '47.2.0',
      resolved: { kind: 'forge', version: '47.2.0' },
    }),
  );
});
```

`ui/src/features/new-instance/NewInstance.test.tsx`: add the following. The mock backend's `catalog_loaders` returns the same fixture list for every loader, so the newest stable is `0.19.5`.

```tsx
it('offers Forge and NeoForge and creates a Forge instance', async () => {
  const { user, calls } = renderApp(<NewInstance />);
  await screen.findByRole('option', { name: '26.3' });
  expect(screen.getByRole('radio', { name: 'NeoForge' })).toBeInTheDocument();
  await user.click(screen.getByRole('radio', { name: 'Forge' }));
  expect(await screen.findByRole('combobox', { name: 'Loader version' })).toHaveValue('0.19.5');
  await user.click(screen.getByRole('button', { name: 'Create instance' }));
  expect(sent(calls, 'instance_create')).toEqual([
    { request: { name: 'Minecraft 26.3', minecraft: '26.3', loader: { kind: 'forge', version: '0.19.5' } } },
  ]);
});
```

`ui/src/features/instance-settings/InstanceSettings.test.tsx`: replace `'saves a Forge instance with its game unchanged and explains that the version cannot change yet'` with:

```tsx
it('shows a Forge instance on Forge and saves its game unchanged', async () => {
  const forge = { minecraft: '1.20.1', loader: { kind: 'forge', version: '47.2.0' } } as const;
  const { user, calls, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  backend.state.listing.instances[0]!.game = structuredClone(forge);
  expect(await screen.findByRole('radio', { name: 'Forge' })).toBeChecked();
  expect(await screen.findByRole('combobox', { name: 'Loader version' })).toHaveValue('47.2.0');
  await user.click(await screen.findByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { game: unknown } };
  expect(saved.instance.game).toEqual(forge);
});
```

`ui/src/features/mods/Mods.test.tsx`: change `/Mods need Fabric or Quilt/` to `/Mods need a mod loader/`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-app --lib app::` and `pnpm --filter ui test`
Expected: the Rust test fails with `LoaderUnsupported`. The four UI tests fail: no Forge radio, loader `'none'`, and the old vanilla note.

- [ ] **Step 3: Implement**

`crates/launcher-app/src/app.rs`: delete the `if let Some(loader) = &request.loader { if matches!(… Forge | NeoForge) { return Err(…) } }` block at the top of `instance_create`. Remove imports that become unused.

`ui/src/features/new-instance/useVersionChoice.ts`:

```ts
import type { GameSpec, LoaderKind, LoaderSpec } from '../../ipc/schema';

export type LoaderChoice = 'none' | LoaderKind;
```

and replace the two `initialKind` lines (and their comment) with:

```ts
  const initialKind = initial?.loader?.kind ?? null;
```

`ui/src/features/new-instance/VersionFields.tsx`:

```tsx
const LOADERS: LoaderChoice[] = ['none', 'fabric', 'quilt', 'forge', 'neoforge'];
const LOADER_KEY = {
  none: 'newInstance.loaderNone',
  fabric: 'newInstance.loaderFabric',
  quilt: 'newInstance.loaderQuilt',
  forge: 'newInstance.loaderForge',
  neoforge: 'newInstance.loaderNeoForge',
} as const;
```

`ui/src/features/instance-settings/InstanceSettings.tsx`: delete `FIXED_LOADERS` and `fixedLoader`. Then:
- `ready` becomes `choice.resolved.ready && (javaMode === 'auto' || javaPath !== null)`.
- `game` becomes `{ minecraft: choice.resolved.minecraft ?? instance.game.minecraft, loader: choice.resolved.loader }`.
- The JSX renders `<VersionFields choice={versions} />` unconditionally.
- Drop the `LoaderKind` import if it becomes unused.

`ui/src/i18n/en.json`:
- In `newInstance`, add `"loaderForge": "Forge"` and `"loaderNeoForge": "NeoForge"` after `loaderQuilt`.
- In `mods`, set `"vanillaNote": "Mods need a mod loader: Fabric, Quilt, Forge or NeoForge. You can switch this instance's loader in its settings."`.
- Delete `instanceSettings.loaderFixed`.
- Change `errors.loader_unsupported.fix` to `"Pick another loader."`.

`ui/src/i18n/es.json`:
- In `newInstance`, add `"loaderForge": "Forge"` and `"loaderNeoForge": "NeoForge"`.
- In `mods`, set `"vanillaNote": "Los mods necesitan un loader: Fabric, Quilt, Forge o NeoForge. Puedes cambiar el loader de esta instancia en su configuración."`.
- Delete `instanceSettings.loaderFixed`.
- Change `errors.loader_unsupported.fix` to `"Elige otro loader."`.

- [ ] **Step 4: Run all checks**

```bash
cargo test -p launcher-app
pnpm --filter ui build && pnpm -r lint && pnpm -r typecheck && pnpm -r test
```

Expected: all PASS.

- [ ] **Step 5: Look at it**

Start the mocked UI (`preview_start` with the `ui` dev:mock configuration in `.claude/launch.json`; create one running `pnpm --filter ui dev:mock` if it's missing). Open New instance. Check that the five loader segments sit on one or two tidy rows at the default window width and at 900 px wide. Then pick Forge and confirm the version select appears. Take a screenshot for the PR.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/launcher-app ui/src
git commit -m "feat(app): offer Forge and NeoForge when creating or editing an instance" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: CLI and nightly CI

`perland-cli` is a dev harness with no tests (CLAUDE.md). It is verified by running it, here and in Task 11.

**Files:**
- Modify: `crates/perland-cli/src/main.rs`
- Modify: `.github/workflows/nightly.yml`

**Interfaces:**
- Consumes:
  - `LoaderKind::{Forge, NeoForge}`.
  - `java::detect_installed` (Task 3).
  - `launch::{command_line_len, ARGFILE_THRESHOLD}` (Task 7).

- [ ] **Step 1: Loader flags, `java list`, dry-run length**

```rust
#[derive(Clone, Copy, ValueEnum)]
enum CliLoader {
    Fabric,
    Quilt,
    Forge,
    #[value(name = "neoforge")]
    NeoForge,
}
```

Map the new variants in `Cmd::Create`: `CliLoader::Forge => LoaderKind::Forge, CliLoader::NeoForge => LoaderKind::NeoForge`.

Add a subcommand:

```rust
    /// Java installations
    Java {
        #[command(subcommand)]
        cmd: JavaCmd,
    },
```

```rust
#[derive(Subcommand)]
enum JavaCmd {
    /// List the Java installations found on this computer
    List,
}
```

handled as:

```rust
        Cmd::Java { cmd: JavaCmd::List } => {
            for java in launcher_core::java::detect_installed(Platform::current()).await {
                println!("{}\t{}", java.major, java.path.display());
            }
        }
```

In the dry-run branch, after `play::check_files(&cmd)?;`:

```rust
        let len = launcher_core::launch::command_line_len(&cmd);
        eprintln!(
            "command line: {len} characters{}",
            if len > launcher_core::launch::ARGFILE_THRESHOLD {
                " (a real launch puts the JVM arguments in an argfile)"
            } else {
                ""
            }
        );
```

- [ ] **Step 2: Try it**

```bash
cargo run -q -p perland-cli -- --help
cargo run -q -p perland-cli -- create --help
cargo run -q -p perland-cli -- java list
```

Expected: `create --help` lists `fabric, quilt, forge, neoforge`. `java list` prints at least `25	C:\Program Files\Eclipse Adoptium\jdk-25…\bin\javaw.exe` on this PC.

- [ ] **Step 3: Nightly CI** (in `.github/workflows/nightly.yml`, after the Fabric step)

```yaml
      - name: Install Forge 1.20.1-47.4.22 (installer processors), then launch dry-run
        shell: bash
        run: |
          ROOT="$RUNNER_TEMP/perland"
          ID=$(cargo run -q -p perland-cli -- --root "$ROOT" create --name "CI Forge" --mc 1.20.1 --loader forge --loader-version 47.4.22)
          cargo run -q -p perland-cli -- --root "$ROOT" launch --instance "$ID" --dry-run
          test -f "$ROOT/libraries/net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-client.jar"
          test -f "$ROOT/versions/1.20.1-forge-47.4.22/1.20.1-forge-47.4.22.jar"
```

The existing "Second run is offline-capable" step picks the first listed instance. Make it cover Forge explicitly by replacing its body with:

```yaml
        run: |
          ROOT="$RUNNER_TEMP/perland"
          for ID in $(cargo run -q -p perland-cli -- --root "$ROOT" list | cut -f1); do
            time cargo run -q -p perland-cli -- --root "$ROOT" launch --instance "$ID" --dry-run
          done
```

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/perland-cli .github/workflows/nightly.yml
git commit -m "feat(cli): create Forge/NeoForge instances, list installed Java; nightly Forge dry run" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: End-to-end verification, owner launch, PR

This is a normal download (about 1 GB with assets), not a stress test. It uses a scratch root inside `target/`, which is git-ignored.

- [ ] **Step 1: Real Forge 1.20.1-47.4.22 install (Asalith's loader), dry run**

```bash
cargo build -q -p perland-cli
cargo run -q -p perland-cli -- --root target/pl-forge offline Tester
cargo run -q -p perland-cli -- --root target/pl-forge create --name "Asalith Fields" --mc 1.20.1 --loader forge --loader-version 47.4.22
cargo run -q -p perland-cli -- --root target/pl-forge launch --instance asalith-fields --dry-run
```

Expected:
- Progress for `install_minecraft`, `download_java` and `install_loader` (6 steps).
- The printed command contains `cpw.mods.bootstraplauncher.BootstrapLauncher`, `-DignoreList=…,1.20.1-forge-47.4.22.jar` and `--launchTarget forgeclient`.
- `command line: N characters`. Record N in the PR.
- `dry run OK`.
- `target/pl-forge/libraries/net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-client.jar` exists.

- [ ] **Step 2: Second run skips the processors**

Run the same `launch --dry-run` again.
Expected: no `install_loader` processor progress beyond the instant "done", and it finishes in seconds.

- [ ] **Step 3: NeoForge spot check**

```bash
cargo run -q -p perland-cli -- --root target/pl-forge create --name "Neo" --mc 1.21.1 --loader neoforge --loader-version 21.1.257
cargo run -q -p perland-cli -- --root target/pl-forge launch --instance neo --dry-run
```

Expected: `dry run OK`, with `-DignoreList=client-extra,neoforge-21.1.257.jar` in the command.

- [ ] **Step 4: Ask the owner for the real launch.** An agent can't close the game window. Give them:

```bash
cargo run -q -p perland-cli -- --root target/pl-forge launch --instance asalith-fields
```

Ask them to confirm three things: the Forge title screen loads, the Mods list shows Forge and Minecraft, and a singleplayer world loads. Fix anything they report before the PR (superpowers:systematic-debugging).

- [ ] **Step 5: Final checks, rebase, PR**

```bash
git fetch origin
git rebase origin/main
pnpm --filter ui build && cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm -r lint && pnpm -r typecheck && pnpm -r test
git push -u origin HEAD
gh pr create --base main --title "Forge and NeoForge (Plan 2)" --body-file <file>
```

The PR body covers:
- What each task delivered.
- The dry-run evidence: command length, file checks and the second-run time.
- The owner's launch result.
- The NeoForge spot check.
- The `ResolvedVersion::forge` field addition. Other sessions constructing `ResolvedVersion` must add `forge: None`.
- The body ends with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

After opening it, bind the PR with the ccd_pr tools and report CI.
