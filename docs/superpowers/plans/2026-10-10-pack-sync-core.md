# Pack Sources and Sync, Core + CLI (Plan 3a of 6) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `launcher-core` turns a packwiz URL, a `.mrpack` file or a Modrinth modpack into an instance and keeps it in sync safely: player files survive, an interrupted sync never breaks the instance, and CurseForge files that can't be downloaded fall back to a manual flow. `perland-cli` exposes all of it (`pack add`, `pack check`, `sync`, and `launch` syncs first), verified against the real Asalith Fields pack.

**Architecture:**
- **`sources`:** every pack reduces to a `DesiredState`. It is resolved over HTTP through `MetaClient`, whose hash-verified cache makes resolving work offline. Remote pack files land in a content-addressed `ObjectCache`.
- **`sync`:**
  1. Diff the `DesiredState` against `.perland/managed.json` and the files on disk with a pure decision table.
  2. Stage every file to write in `.perland/staging/`.
  3. Install a changed Minecraft or loader version first.
  4. Swap the files in, moving anything replaced or removed into `.perland/trash/`.
  5. Write `.perland/pack.json` (the installed `DesiredState`), then `managed.json` last.
- **Locks:** an OS file lock per instance guards sync, and a second file lock serializes installs across processes.

**Tech Stack:**
- Rust stable (1.99, edition 2021): tokio, reqwest (rustls), serde/serde_json, zip 2, thiserror.
- New dependencies: `toml` 1, `fs4` 1.1 (free disk space), `dirs` 7 (Downloads folder), `percent-encoding` 2.
- Tests use tempfile and wiremock. Fuzzing uses cargo-fuzz, in CI only.

**Spec:**
- `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (§2.1, §2.3, §3, §6, §7).
- The addendum with the split, the owner's decisions and the decisions where the spec is silent: `docs/superpowers/specs/2026-10-10-pack-sync-design.md`. **Read both.**
- Plan 3b (desktop app) and 3c (Browse) are separate plans.

**Owner decisions for this plan (2026-10-10):** see the addendum §2.
- Split 3a/3b/3c.
- "ask" shows a dialog on Play (that's 3b; in 3a the CLI treats `ask` like `auto`).
- `launcher.toml` `[sync] policy = "auto"` makes a pack default to `auto`.
- Short, casual copy.

## Global Constraints

- **License:** GPL-3.0-only. No Modrinth or Mojang branding or assets. Everything here is written fresh from the published formats (packwiz `pack.toml`/`index.toml`/`.pw.toml`, Modrinth's `.mrpack` and API v2); nothing is ported from Modrinth's app-lib, so `NOTICE` doesn't change. If an implementer does port code, that file gets an attribution header and a `NOTICE` entry.
- **Dependencies:** `launcher-core` and `launcher-app` must not depend on Tauri or any UI crate. `src-tauri` is wiring only and is not touched by this plan.
- **Platforms:** Rust stable, edition 2021, on Windows x64 and Linux x64.
- **Upstream addresses:**
  - Modrinth: `https://api.modrinth.com/v2`. No key; requests send the existing descriptive `User-Agent` from `http::client`.
  - CurseForge CDN: `https://mediafilez.forgecdn.net/files/{id/1000}/{id%1000}/{percent-encoded file name}`.
  - CurseForge page: `https://www.curseforge.com/projects/{project-id}`.
  - All of these are overridable through `SourceConfig` for tests.
- **Untrusted input:** pack links, pack paths, Modrinth ids and archive entry names are untrusted.
  - Every path from a pack goes through `fsutil::normalize_rel` / `safe_join`.
  - Pack links must be `https://`. `http://` is accepted only for loopback hosts, which tests use.
  - Cache keys are built only from validated hex.
- **Errors:**
  - Player-facing failures are `LauncherError` (category + stable `ErrorCode` + plain message + optional fix + detail + params).
  - Every new `ErrorCode` gets `errors.<code>.message` in both `ui/src/i18n/en.json` and `ui/src/i18n/es.json` (neutral Latin American Spanish, informal "tú"; the existing files say "pack").
  - Then regenerate the contract: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit `ui/src/ipc/contract/*.json`.
- **No URLs in player messages:** messages name the host only. `detail` may carry a URL with its query, fragment and userinfo removed (`download::redact_url`). Tokens never reach logs.
- **Paths for Java:** every path handed to Java is absolute (`LauncherDirs::new` already makes the root absolute).
- **Stable APIs:** another session (Plan 4, Microsoft sign-in) may be working in `accounts`/`auth` and the Accounts screen. Don't touch those, keep existing public signatures, and prefer additive changes. The deliberate breaks:
  - `SourceRef::Mrpack` becomes `{ name, sha1 }`. No instance uses it yet.
  - `Hashes` gains `md5`. Struct literals in this repo are updated.
  
  Rebase onto `main` before the PR.
- **Heavy tests:** fuzzing, stress and load tests run only in CI (nightly), **never on this PC**. Don't run `cargo fuzz` locally, not even to "try it". Local test inputs stay tiny; size limits are tested through parameters, never with big data. The real Asalith sync in Task 21 is a normal download.
- **Commits:** every commit message ends with the trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Before each commit:**
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - When `ui/` changed, also: `pnpm --filter ui build`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.
- **Machine notes:**
  - `cargo` is at `%USERPROFILE%\.cargo\bin`. There is no Python on this PC; use Node 24 for scripts.
  - Windows Smart App Control sometimes blocks a fresh unsigned test exe (os error 4551). That's environmental; retry.

## Review Focus

1. **An interrupted sync** (crash, power loss or network drop at any point before `managed.json` is written).
   - **Expected:** the previous lockfile still describes a launchable state, and the next sync finishes from the object cache without downloading finished files again.
   - **Pinned by:** `interrupted_sync_keeps_the_old_state_and_resumes_from_the_cache` (Task 18) and `failed_game_install_leaves_the_instance_untouched` (Task 15).
2. **The player's own things in a pack instance:** jars they added, `preserve` configs they edited, `.mrpack` override files they changed.
   - **Expected:** they survive every update.
   - A player file at a path the pack now claims is replaced but kept in the trash.
   - Managed files that the pack drops go to the trash, never straight to deletion.
   - **Pinned by:** `update_keeps_additions_and_preserved_configs` (Task 18), the planner's override-rule tests (Task 13) and `mrpack_override_edits_survive_an_update` (Task 18).
3. **A hostile pack:**
   - **Inputs:** `../`, absolute or drive paths, `a.jar:stream`, `CON.jar`, `mods/a.jar` next to `MODS/A.JAR`, a cache key that isn't hex, a zip entry larger than it claims.
   - **Expected:** nothing is written outside the instance's game folder or the cache, and the pack fails with `pack_unsafe_path`/`pack_invalid`.
   - **Pinned by:** `normalize_rel_rejects_unsafe_parts` (Task 2), `cache_key_ignores_values_that_are_not_hex` (Task 3), `packwiz_rejects_escaping_and_colliding_paths` (Task 9), `mrpack_rejects_unsafe_paths_and_oversized_entries` (Task 10).
4. **A pasted or deep-linked `http://` pack URL** (MITM-able: mods are code).
   - **Expected:** refused with `pack_link_invalid`, except loopback hosts used by tests.
   - **Pinned by:** `pack_input_requires_https_except_loopback` (Task 7).
5. **Play while offline.**
   - **Expected:** the update check fails softly with an `update_check_failed` warning, and the instance syncs from its installed snapshot and the cache. A pack that was never synced fails with a clear `offline`/`pack_not_found` error, not a half-made instance.
   - **Pinned by:** `offline_ensure_uses_the_installed_snapshot_with_a_warning` (Task 17).

## File Map

| File | Responsibility |
|---|---|
| `crates/launcher-core/src/error.rs` | 10 new `ErrorCode`s |
| `crates/launcher-core/src/fsutil.rs` | `normalize_rel`, `safe_component`, `path_key`, hardened `safe_join` |
| `crates/launcher-core/src/download/hash.rs` | `Hashes.md5`, `from_format`, `cache_key`, `sha1_file`, murmur2 tests |
| `crates/launcher-core/src/download/mod.rs` | host-only messages, `redact_url`, normalized and pruned destination locks |
| `crates/launcher-core/src/download/cache.rs` | `ObjectCache` (content-addressed `cache/objects`) |
| `crates/launcher-core/src/locks.rs` | `InstanceLock` (try), `InstallLock` (blocking), both OS file locks |
| `crates/launcher-core/src/install/{mod,natives}.rs` | install lock around `install`; natives marker deleted first |
| `crates/launcher-core/src/meta/mod.rs` | `get_verified_bytes`; host-only messages |
| `crates/launcher-core/src/paths.rs` | `objects()`, `install_lock()` |
| `crates/launcher-core/src/instances.rs` | `SourceRef::Mrpack { name, sha1 }` |
| `crates/launcher-core/src/sources/mod.rs` | `DesiredState` and friends, `PackInput`, `SourceConfig`, `UpdateCheck`, the `Sources` facade |
| `crates/launcher-core/src/sources/{curseforge,sidecar,packwiz,mrpack,modrinth,art}.rs` | one source format each, plus art download |
| `crates/launcher-core/src/sync/lockfile.rs` | `managed.json`, `pack.json`, "Your additions" |
| `crates/launcher-core/src/sync/plan.rs` | the pure §3.2 decision table and the changelog |
| `crates/launcher-core/src/sync/apply.rs` | survey, free space, trash, file moves |
| `crates/launcher-core/src/sync/manual.rs` | finding manually downloaded CurseForge files |
| `crates/launcher-core/src/sync/mod.rs` | `Syncer`: sync, ensure (policies), add_pack, preview, manual import |
| `crates/launcher-core/tests/common/mod.rs` | `PackBuilder` (a tiny packwiz pack on wiremock), `sources()`, zip helper |
| `crates/launcher-core/tests/{cache,packwiz,mrpack,modrinth,sync}.rs` | integration tests |
| `crates/perland-cli/src/main.rs` | `pack add`, `pack check`, `sync`, `launch` syncs first |
| `fuzz/` | cargo-fuzz targets (own workspace; CI only) |
| `.github/workflows/nightly.yml` | real Asalith sync + dry-run; fuzz job |
| `ui/src/i18n/{en,es}.json`, `ui/src/ipc/schema.ts`, `ui/src/ipc/contract/*` | translations, `SourceRef` schema, regenerated contract |

---

### Task 1: Error codes, the `.mrpack` source shape, translations, contract

Every later task uses these codes, so they land first, together with the one UI-visible type change.

**Files:**
- Modify: `crates/launcher-core/src/error.rs` (the `error_codes!` list)
- Modify: `crates/launcher-core/src/instances.rs` (`SourceRef::Mrpack`, plus a test)
- Modify: `crates/launcher-core/src/meta/loaders.rs` (stale "isn't supported yet" text and its test)
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json`, `ui/src/ipc/schema.ts:21`
- Regenerate: `ui/src/ipc/contract/error_codes.json`

**Interfaces:**
- Produces these `ErrorCode` variants (snake_case on the wire): `PackInvalid`, `PackUnsupported`, `PackUnsafePath`, `PackNotFound`, `PackLinkInvalid`, `NoPack`, `NotEnoughSpace`, `InstanceLocked`, `ManualDownloadsNeeded`, `UpdateCheckFailed`.
- Produces `SourceRef::Mrpack { name: String, sha1: String }`, serialized as `{"kind":"mrpack","name":…,"sha1":…}`.
- Produces the i18n key `progress.sync_pack` ("Updating {{name}}").

- [ ] **Step 1: Write the failing tests**

In `crates/launcher-core/src/instances.rs` `mod tests`, next to `modrinth_source_uses_camel_case`:

```rust
    #[test]
    fn mrpack_source_names_the_cached_archive() {
        let s = SourceRef::Mrpack {
            name: "Pack-1.0.mrpack".into(),
            sha1: "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into(),
        };
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"kind":"mrpack","name":"Pack-1.0.mrpack","sha1":"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"}"#
        );
    }
```

In `crates/launcher-core/src/error.rs` `mod tests`:

```rust
    #[test]
    fn pack_codes_exist() {
        for (code, wire) in [
            (ErrorCode::PackInvalid, "pack_invalid"),
            (ErrorCode::PackUnsupported, "pack_unsupported"),
            (ErrorCode::PackUnsafePath, "pack_unsafe_path"),
            (ErrorCode::PackNotFound, "pack_not_found"),
            (ErrorCode::PackLinkInvalid, "pack_link_invalid"),
            (ErrorCode::NoPack, "no_pack"),
            (ErrorCode::NotEnoughSpace, "not_enough_space"),
            (ErrorCode::InstanceLocked, "instance_locked"),
            (ErrorCode::ManualDownloadsNeeded, "manual_downloads_needed"),
            (ErrorCode::UpdateCheckFailed, "update_check_failed"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib -- mrpack_source_names pack_codes_exist`
Expected: compile errors (`no variant named PackInvalid`, `struct variant has no field named name`).

- [ ] **Step 3: Implement**

In `error.rs`, add a group before `// anything unexpected`:

```rust
    // packs and sync
    PackInvalid, PackUnsupported, PackUnsafePath, PackNotFound, PackLinkInvalid, NoPack,
    NotEnoughSpace, InstanceLocked, ManualDownloadsNeeded, UpdateCheckFailed,
```

In `instances.rs`, replace the `Mrpack` variant and the enum's doc comment:

```rust
/// Where an instance's content comes from. `Mrpack` is an imported file, kept in the object
/// cache under its sha1; `name` is only for display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SourceRef {
    None,
    Packwiz {
        url: String,
    },
    Mrpack {
        name: String,
        sha1: String,
    },
    #[serde(rename_all = "camelCase")]
    Modrinth {
        project_id: String,
        version_id: String,
    },
}
```

In `meta/loaders.rs`, change the `LoaderUnsupported` message from `"{…} isn't supported yet"` to `"{…} isn't supported"`, and update the assertion at line ~152 to `contains("isn't supported")`.

In `ui/src/ipc/schema.ts`, line 21 becomes:

```ts
  z.strictObject({ kind: z.literal('mrpack'), name: z.string(), sha1: z.string() }),
```

In `ui/src/i18n/en.json`:
- Add `"sync_pack": "Updating {{name}}"` under `progress`.
- Change `errors.loader_unsupported` to `{ "message": "{{loader}} isn't supported.", "fix": "Pick another loader." }`.
- Add under `errors`:

```json
    "pack_invalid": { "message": "The pack file {{file}} is damaged or isn't a pack.", "fix": "Check the link, or ask the pack's author." },
    "pack_unsupported": { "message": "This pack uses something the launcher can't handle yet: {{what}}.", "fix": "Ask the pack's author, or try another pack." },
    "pack_unsafe_path": { "message": "The pack tried to write outside its folder ({{path}}).", "fix": "Don't use this pack, and let its author know." },
    "pack_not_found": { "message": "That pack wasn't found.", "fix": "Check the link and try again." },
    "pack_link_invalid": { "message": "That doesn't look like a pack link.", "fix": "Pack links start with https:// and usually end in pack.toml." },
    "no_pack": { "message": "This instance doesn't come from a pack." },
    "not_enough_space": { "message": "Not enough free space: {{needed_mb}} MB needed, {{free_mb}} MB free.", "fix": "Free up some space and try again." },
    "instance_locked": { "message": "Another Perland Launcher window is using this instance.", "fix": "Close the other window, or wait for it to finish." },
    "manual_downloads_needed": { "message": "{{count}} files have to be downloaded by hand.", "fix": "Download them from CurseForge, then try again." },
    "update_check_failed": { "message": "Couldn't check for updates.", "fix": "You can still play the version you have." }
```

In `ui/src/i18n/es.json`:
- Add `"sync_pack": "Actualizando {{name}}"` under `progress`.
- Change `errors.loader_unsupported` to `{ "message": "{{loader}} no es compatible.", "fix": "Elige otro loader." }`.
- Add under `errors`:

```json
    "pack_invalid": { "message": "El archivo {{file}} del pack está dañado o no es un pack.", "fix": "Revisa el enlace o pregúntale al autor del pack." },
    "pack_unsupported": { "message": "Este pack usa algo que el launcher todavía no puede manejar: {{what}}.", "fix": "Pregúntale al autor del pack o prueba otro pack." },
    "pack_unsafe_path": { "message": "El pack intentó escribir fuera de su carpeta ({{path}}).", "fix": "No uses este pack y avísale a su autor." },
    "pack_not_found": { "message": "No se encontró ese pack.", "fix": "Revisa el enlace e inténtalo de nuevo." },
    "pack_link_invalid": { "message": "Eso no parece un enlace de pack.", "fix": "Los enlaces de pack empiezan con https:// y suelen terminar en pack.toml." },
    "no_pack": { "message": "Esta instancia no viene de un pack." },
    "not_enough_space": { "message": "No hay suficiente espacio libre: se necesitan {{needed_mb}} MB y hay {{free_mb}} MB.", "fix": "Libera espacio e inténtalo de nuevo." },
    "instance_locked": { "message": "Otra ventana de Perland Launcher está usando esta instancia.", "fix": "Cierra la otra ventana o espera a que termine." },
    "manual_downloads_needed": { "message": "Hay {{count}} archivos que tienes que descargar a mano.", "fix": "Descárgalos de CurseForge e inténtalo de nuevo." },
    "update_check_failed": { "message": "No se pudieron buscar actualizaciones.", "fix": "Igual puedes jugar la versión que tienes." }
```

(Keep the existing order style of each file: append the new error keys at the end of `errors`.)

- [ ] **Step 4: Regenerate the contract and run everything**

Run:
```bash
UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract
cargo test -p launcher-core --lib -- mrpack_source_names pack_codes_exist loaders
cargo test -p launcher-app --test contract
pnpm -r typecheck && pnpm -r test
```
Expected:
- The two new tests pass.
- The contract test passes, and `ui/src/ipc/contract/error_codes.json` gained the 10 codes.
- The UI i18n tests pass (key parity, placeholders, and every code translated).

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/error.rs crates/launcher-core/src/instances.rs crates/launcher-core/src/meta/loaders.rs ui/src/i18n ui/src/ipc/schema.ts ui/src/ipc/contract
git commit -m "feat(core): error codes for packs and sync; .mrpack sources name their cached archive

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Safe pack paths

Pack paths are untrusted. `safe_join` currently accepts `.`, `a/.`, NTFS `:` streams and Windows device names. One function now validates and canonicalizes a relative path, and `safe_join` uses it, which also hardens the existing callers: installer, Java runtime and mods.

**Files:**
- Modify: `crates/launcher-core/src/fsutil.rs`

**Interfaces:**
- Produces:
  - `pub fn normalize_rel(rel: &str) -> Option<String>`: the canonical `/`-separated form, or `None` when unsafe.
  - `pub fn safe_component(part: &str) -> bool`
  - `pub fn path_key(rel: &str) -> String`: the case-folded comparison key for collision checks.
  - `pub fn safe_join(base: &Path, rel: &str) -> Option<PathBuf>`: the signature is unchanged and it now joins the canonical form.

- [ ] **Step 1: Write the failing tests** (replace `safe_join_accepts_relative_paths` and add the rest to `fsutil.rs` `mod tests`)

```rust
    #[test]
    fn safe_join_accepts_relative_paths() {
        let base = Path::new("/base");
        assert_eq!(
            safe_join(base, "bin/javaw.exe"),
            Some(base.join("bin/javaw.exe"))
        );
        assert_eq!(safe_join(base, "a\\b.txt"), Some(base.join("a/b.txt")));
    }

    #[test]
    fn normalize_rel_canonicalizes() {
        assert_eq!(normalize_rel("mods/a.jar").as_deref(), Some("mods/a.jar"));
        assert_eq!(normalize_rel("config\\x.toml").as_deref(), Some("config/x.toml"));
        assert_eq!(normalize_rel("a b/c+d.jar").as_deref(), Some("a b/c+d.jar"));
        // Device names only match the whole stem.
        for ok in ["console.jar", "nullify.cfg", "com10.txt", "auxiliary/x", "CONFIG/x"] {
            assert!(normalize_rel(ok).is_some(), "{ok:?}");
        }
    }

    #[test]
    fn normalize_rel_rejects_unsafe_parts() {
        for bad in [
            "", ".", "..", "./a", "a/.", "a/./b", "a//b", "a/", "/a", "\\a", "../x", "a/../../x",
            "C:/x", "C:x", "a\0b", "a\u{1}b",
            "mods/a.jar:evil", "a.jar:Zone.Identifier",
            "CON", "con.txt", "mods/NUL.jar", "Com1.log", "lpt9", "conin$", "CONOUT$.x",
            "a/aux .txt", "com¹", "trail.", "trail ", "mods/x./y",
            "a<b", "a>b", "a|b", "a?b", "a*b", "a\"b",
        ] {
            assert_eq!(normalize_rel(bad), None, "{bad:?} must be rejected");
            assert_eq!(safe_join(Path::new("/base"), bad), None, "{bad:?}");
        }
        assert!(normalize_rel(&"a/".repeat(600)).is_none(), "overlong paths are refused");
    }

    #[test]
    fn path_key_folds_case() {
        assert_eq!(path_key("Mods/A.jar"), path_key("mods/a.JAR"));
        assert_ne!(path_key("mods/a.jar"), path_key("mods/b.jar"));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib fsutil`
Expected: compile error, `cannot find function normalize_rel`.

- [ ] **Step 3: Implement.** Replace `safe_join` and its doc comment in `fsutil.rs`:

```rust
/// Windows device names. A file whose stem is one of these (`nul.txt`, `CON .jar`) opens the
/// device instead of a file, so such names are never accepted from remote data.
const DEVICE_NAMES: [&str; 30] = [
    "con", "prn", "aux", "nul", "conin$", "conout$", "com1", "com2", "com3", "com4", "com5",
    "com6", "com7", "com8", "com9", "com¹", "com²", "com³", "lpt1", "lpt2", "lpt3", "lpt4",
    "lpt5", "lpt6", "lpt7", "lpt8", "lpt9", "lpt¹", "lpt²", "lpt³",
];

/// Longest relative path accepted from remote data, in bytes.
const MAX_REL_LEN: usize = 1024;

/// One path segment that is safe on every platform: not empty, `.` or `..`; no `:` (drive
/// letters, NTFS alternate streams), no `<>"|?*` or control characters; no trailing dot or
/// space (Windows strips them, so two names would alias); and not a Windows device name.
pub fn safe_component(part: &str) -> bool {
    if part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' ']) {
        return false;
    }
    if part
        .chars()
        .any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*' | '/' | '\\'))
    {
        return false;
    }
    let stem = part
        .split('.')
        .next()
        .unwrap_or(part)
        .trim_end()
        .to_lowercase();
    !DEVICE_NAMES.contains(&stem.as_str())
}

/// A remote-supplied relative path in canonical form (segments joined with `/`), or `None`
/// when any segment is unsafe (see [`safe_component`]). Splits on both `/` and `\`, so the
/// answer is the same on every platform. Leading, trailing and doubled separators are refused.
pub fn normalize_rel(rel: &str) -> Option<String> {
    if rel.is_empty() || rel.len() > MAX_REL_LEN {
        return None;
    }
    let parts: Vec<&str> = rel.split(['/', '\\']).collect();
    if !parts.iter().all(|p| safe_component(p)) {
        return None;
    }
    Some(parts.join("/"))
}

/// The key two relative paths collide under on a case-insensitive file system.
pub fn path_key(rel: &str) -> String {
    rel.to_lowercase()
}

/// Join a remote-supplied relative path onto `base` without letting it escape or alias:
/// `base` joined with [`normalize_rel`]'s canonical form, or `None` when it is unsafe.
pub fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    normalize_rel(rel).map(|n| base.join(n))
}
```

The existing `safe_join_rejects_escaping_paths` test stays and keeps passing.

- [ ] **Step 4: Run the tests, including every existing caller's tests**

Run: `cargo test -p launcher-core`
Expected: PASS. The installer, Java runtime, Forge and mods tests all still pass: their real paths contain none of the refused forms.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/fsutil.rs
git commit -m "fix(core): refuse dot segments, NTFS streams and device names in remote paths

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Hashes: md5, packwiz hash formats, cache keys, murmur2 tails

**Files:**
- Modify: `crates/launcher-core/src/download/hash.rs`
- Modify: `crates/launcher-core/src/install/mod.rs:127` (the one `Hashes { … }` literal outside `hash.rs`, which gains `md5: None`)

**Interfaces:**
- Produces:
  - `Hashes.md5: Option<String>`, verified by `verify` and `verify_file`.
  - `pub fn md5_hex(data: &[u8]) -> String`
  - `Hashes::is_known_format(format: &str) -> bool`: true for `sha1`, `sha256`, `sha512`, `md5` and `murmur2`, case-insensitively.
  - `Hashes::from_format(format: &str, value: &str) -> Option<Hashes>`: a single-hash `Hashes`, or `None` for an unknown format or a malformed value. Hex is lowercased; murmur2 is decimal (negative 32-bit values wrap).
  - `Hashes::cache_key(&self) -> Option<String>`: the first well-formed value among sha1, sha256, sha512 and md5 (lowercase hex of the right length), else `m2-<murmur2>`.
  - `pub fn sha1_file(path: &Path) -> std::io::Result<String>`: streamed.

- [ ] **Step 1: Write the failing tests** (`hash.rs` `mod tests`)

```rust
    #[test]
    fn murmur2_tail_lengths_and_mismatch() {
        // Values cross-checked against an independent implementation.
        assert_eq!(curseforge_murmur2(b"a"), 626045324, "1-byte tail");
        assert_eq!(curseforge_murmur2(b"ab"), 1692487918, "2-byte tail");
        assert_eq!(curseforge_murmur2(b"abc"), 1621425345, "3-byte tail");
        assert_eq!(curseforge_murmur2(b" a\n"), 626045324, "whitespace never counts");
        let h = Hashes {
            murmur2: Some(626045324),
            ..Default::default()
        };
        assert!(h.verify(b"a"));
        assert!(!h.verify(b"b"), "a different file must not match");
    }

    #[test]
    fn md5_is_verified() {
        assert_eq!(md5_hex(b"hello"), "5d41402abc4b2a76b9719d911017c592");
        let h = Hashes::from_format("md5", "5D41402ABC4B2A76B9719D911017C592").unwrap();
        assert_eq!(h.md5.as_deref(), Some("5d41402abc4b2a76b9719d911017c592"));
        assert_agrees(&h, b"hello", true);
        assert_agrees(&h, b"jello", false);
    }

    #[test]
    fn from_format_validates_values() {
        assert!(Hashes::is_known_format("SHA256"));
        assert!(!Hashes::is_known_format("crc32"));
        assert_eq!(Hashes::from_format("crc32", "00"), None);
        assert_eq!(Hashes::from_format("sha1", "xyz"), None, "not hex");
        assert_eq!(Hashes::from_format("sha1", &"a".repeat(39)), None, "wrong length");
        assert_eq!(
            Hashes::from_format("sha256", &sha256_hex(b"hello")).unwrap().sha256,
            Some(sha256_hex(b"hello"))
        );
        assert_eq!(
            Hashes::from_format("murmur2", "2824650221").unwrap().murmur2,
            Some(2824650221)
        );
        assert_eq!(
            Hashes::from_format("murmur2", "-1").unwrap().murmur2,
            Some(u32::MAX),
            "signed 32-bit fingerprints wrap"
        );
        assert_eq!(Hashes::from_format("murmur2", "9999999999"), None);
    }

    #[test]
    fn cache_key_prefers_sha1_then_sha256() {
        let both = Hashes {
            sha1: Some(sha1_hex(b"x").to_uppercase()),
            sha256: Some(sha256_hex(b"x")),
            ..Default::default()
        };
        assert_eq!(both.cache_key(), Some(sha1_hex(b"x")));
        let sha512 = Hashes {
            sha512: Some(sha512_hex(b"x")),
            ..Default::default()
        };
        assert_eq!(sha512.cache_key(), Some(sha512_hex(b"x")));
        let m2 = Hashes {
            murmur2: Some(7),
            ..Default::default()
        };
        assert_eq!(m2.cache_key().as_deref(), Some("m2-7"));
        assert_eq!(Hashes::default().cache_key(), None);
    }

    #[test]
    fn cache_key_ignores_values_that_are_not_hex() {
        // Hashes from a pack are untrusted, and the key becomes a file name.
        assert_eq!(Hashes::sha1("../../evil").cache_key(), None);
        let mixed = Hashes {
            sha1: Some("../../evil".into()),
            sha256: Some(sha256_hex(b"x")),
            ..Default::default()
        };
        assert_eq!(mixed.cache_key(), Some(sha256_hex(b"x")));
    }

    #[test]
    fn sha1_file_streams() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(sha1_file(&path).unwrap(), sha1_hex(b"hello"));
    }
```

Also add `md5: None` to the `all_sha` literal in `verify_file_agrees_with_verify`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib download::hash`
Expected: compile errors (`no field md5`, `cannot find function md5_hex`).

- [ ] **Step 3: Implement** in `hash.rs`

- Add `use md5::Md5;` (the `md-5` crate's library name is `md5`; it uses the same `Digest` trait as `sha1`).
- Add the field `pub md5: Option<String>,` after `sha512`.
- Make `is_empty` also require `self.md5.is_none()`.
- In `verify`, add `(&self.md5, md5_hex)` to `checks`, making it a 4-element array.
- In `verify_file`:
  - add `let mut md5 = self.md5.as_ref().map(|_| Md5::new());`
  - update it per chunk like the others
  - `&& matches(&self.md5, md5)` in the result.

Then add:

```rust
pub fn md5_hex(data: &[u8]) -> String {
    hex::encode(Md5::digest(data))
}

/// The sha1 of a file, read in 64 KiB chunks.
pub fn sha1_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn well_formed_hex(value: &str, len: usize) -> Option<String> {
    let v = value.trim().to_ascii_lowercase();
    (v.len() == len && v.bytes().all(|b| b.is_ascii_hexdigit())).then_some(v)
}

impl Hashes {
    pub fn is_known_format(format: &str) -> bool {
        matches!(
            format.to_ascii_lowercase().as_str(),
            "sha1" | "sha256" | "sha512" | "md5" | "murmur2"
        )
    }

    /// A one-hash `Hashes` from a pack's `hash-format` and value. `None` for an unknown format
    /// or a malformed value. Hex is lowercased; murmur2 is decimal, and negative 32-bit values
    /// (as some tools write them) wrap to the same fingerprint.
    pub fn from_format(format: &str, value: &str) -> Option<Hashes> {
        let mut h = Hashes::default();
        match format.to_ascii_lowercase().as_str() {
            "sha1" => h.sha1 = Some(well_formed_hex(value, 40)?),
            "sha256" => h.sha256 = Some(well_formed_hex(value, 64)?),
            "sha512" => h.sha512 = Some(well_formed_hex(value, 128)?),
            "md5" => h.md5 = Some(well_formed_hex(value, 32)?),
            "murmur2" => {
                let n: i64 = value.trim().parse().ok()?;
                if !(i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&n) {
                    return None;
                }
                h.murmur2 = Some(n as u32);
            }
            _ => return None,
        }
        Some(h)
    }

    /// The object-cache key: the first well-formed hash among sha1, sha256, sha512 and md5
    /// (their lengths never collide), else `m2-<murmur2>`. Values that aren't hex of the right
    /// length are skipped, so a pack can never steer the key out of the cache folder.
    pub fn cache_key(&self) -> Option<String> {
        [
            (&self.sha1, 40),
            (&self.sha256, 64),
            (&self.sha512, 128),
            (&self.md5, 32),
        ]
        .into_iter()
        .find_map(|(value, len)| value.as_deref().and_then(|v| well_formed_hex(v, len)))
        .or_else(|| self.murmur2.map(|m| format!("m2-{m}")))
    }
}
```

In `install/mod.rs:127`, add `md5: None,` to the `Hashes { … }` literal. If the compiler finds any other complete `Hashes { … }` literal, add `md5: None` there too.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/download/hash.rs crates/launcher-core/src/install/mod.rs
git commit -m "feat(core): md5 and packwiz hash formats, safe cache keys, murmur2 tail tests

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Downloads and metadata name hosts, never full URLs; destination locks are normalized and pruned

Pack URLs can be signed (`?token=…`) or carry user info. Today `DownloadRejected` puts the whole URL in its message, `host_of` keeps `user:pass@`, and `MetaClient` errors quote the URL.

The destination-lock map has two problems:
- It is keyed on the raw `PathBuf`, so `C:/x/A.jar` and `c:\x\a.jar` get two locks.
- It never shrinks.

**Files:**
- Modify: `crates/launcher-core/src/download/mod.rs`
- Modify: `crates/launcher-core/src/meta/mod.rs` (messages; delete its private `host_of`)
- Test: `crates/launcher-core/tests/download.rs`, `crates/launcher-core/tests/meta.rs`, and a new `mod tests` in `download/mod.rs`

**Interfaces:**
- Produces:
  - `pub fn redact_url(url: &str) -> String`: the URL without query, fragment, username or password; `"(not a valid address)"` if it doesn't parse.
  - `pub fn host_of(url: &str) -> String`: the host only, or `"the download server"`.
  
  Both live in `launcher_core::download`.
- `Downloader`'s public API is unchanged.

- [ ] **Step 1: Write the failing tests**

Append to `crates/launcher-core/tests/download.rs`:

```rust
#[tokio::test]
async fn rejected_download_names_only_the_host() {
    let server = MockServer::start().await;
    serve(&server, "/private.jar", 403, "no", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/private.jar?token=SECRET123#frag", server.uri());
    let req = DownloadRequest::new(url, dir.path().join("p.jar"), Hashes::sha1(HELLO_SHA1), Some(5));
    let err = downloader().fetch(&req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::DownloadRejected);
    let all = format!("{} {:?} {:?} {:?}", err.message, err.fix, err.detail, err.params);
    assert!(!all.contains("SECRET123"), "{all}");
    assert!(err.message.contains("127.0.0.1"), "{}", err.message);
    assert_eq!(err.params["status"], "403");
}

#[tokio::test]
async fn unreachable_host_never_shows_user_info() {
    let dir = tempfile::tempdir().unwrap();
    let req = DownloadRequest::new(
        "http://player:hunter2@127.0.0.1:1/x.jar",
        dir.path().join("x.jar"),
        Hashes::sha1(HELLO_SHA1),
        Some(5),
    );
    let err = downloader().fetch(&req).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::HostUnreachable);
    let all = format!("{} {:?} {:?} {:?}", err.message, err.fix, err.detail, err.params);
    assert!(!all.contains("hunter2"), "{all}");
    assert_eq!(err.params["host"], "127.0.0.1");
}
```

(`ErrorCode` is already imported there. If it isn't, add it to the `launcher_core::{…}` import.)

Append to `crates/launcher-core/tests/meta.rs`, adding imports as needed: `launcher_core::meta::MetaClient`, `wiremock::MockServer`.

```rust
#[tokio::test]
async fn meta_errors_never_show_the_query_string() {
    let server = MockServer::start().await; // nothing mounted: every path answers 404
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(reqwest::Client::new(), dir.path());
    let err = meta
        .get_bytes(&format!("{}/pack.toml?token=SECRET123", server.uri()))
        .await
        .unwrap_err();
    let all = format!("{} {:?} {:?} {:?}", err.message, err.fix, err.detail, err.params);
    assert!(!all.contains("SECRET123"), "{all}");
    assert!(err.message.contains("404"), "{}", err.message);
}
```

At the end of `crates/launcher-core/src/download/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn destination_locks_are_forgotten_after_use() {
        let dir = tempfile::tempdir().unwrap();
        let d = Downloader::new(reqwest::Client::new());
        for i in 0..5 {
            let dest = dir.path().join(format!("{i}.txt"));
            std::fs::write(&dest, "hello").unwrap();
            // Already on disk with the right size: no network needed.
            d.fetch(&DownloadRequest::new("http://127.0.0.1:1/x", dest, Hashes::default(), Some(5)))
                .await
                .unwrap();
        }
        assert!(d.locks.lock().unwrap().is_empty());
    }

    #[test]
    fn lock_keys_are_absolute() {
        assert!(lock_key(Path::new("rel/a.jar")).is_absolute());
    }

    #[cfg(windows)]
    #[test]
    fn lock_keys_ignore_case_and_separators_on_windows() {
        assert_eq!(
            lock_key(Path::new("C:/Data/A.jar")),
            lock_key(Path::new("c:\\data\\a.JAR"))
        );
    }

    #[test]
    fn redact_url_drops_secrets() {
        assert_eq!(
            redact_url("https://u:p@cdn.example.com/a/b.jar?sig=x#y"),
            "https://cdn.example.com/a/b.jar"
        );
        assert_eq!(redact_url("not a url"), "(not a valid address)");
        assert_eq!(host_of("https://u:p@cdn.example.com:8443/a"), "cdn.example.com");
        assert_eq!(host_of("nonsense"), "the download server");
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --test download --test meta` and `cargo test -p launcher-core --lib download::tests`
Expected:
- The lib tests fail to compile (`lock_key`/`redact_url` not found).
- `rejected_download_names_only_the_host` fails: the message holds the URL.
- `unreachable_host_never_shows_user_info` fails: the host param is `player:hunter2@127.0.0.1:1`.
- The meta test fails: the message holds the URL.

- [ ] **Step 3: Implement**

In `download/mod.rs`, replace the private `host_of` with:

```rust
/// `url` without its query, fragment and user info: fine for `detail`, never for a message.
pub fn redact_url(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(mut u) => {
            u.set_query(None);
            u.set_fragment(None);
            let _ = u.set_username("");
            let _ = u.set_password(None);
            u.to_string()
        }
        Err(_) => "(not a valid address)".to_string(),
    }
}

/// Just the host of `url`, for player-facing messages and params.
pub fn host_of(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "the download server".to_string())
}

/// The key a destination is locked under: absolute and, on Windows, case-folded, so two
/// spellings of one file share one lock.
fn lock_key(dest: &Path) -> PathBuf {
    let abs = std::path::absolute(dest).unwrap_or_else(|_| dest.to_path_buf());
    if cfg!(windows) {
        PathBuf::from(abs.as_os_str().to_string_lossy().to_lowercase())
    } else {
        abs
    }
}
```

Update every use of `host_of(url)` (it now returns `String`; `format!` and `with_param` accept it as-is). In the `_ =>` arm of `download_to_part`'s status match:

```rust
                return Err(Attempt::NextUrl(
                    LauncherError::network(
                        ErrorCode::DownloadRejected,
                        format!("{} refused the download ({status})", host_of(url)),
                    )
                    .with_param("host", host_of(url))
                    .with_param("status", status)
                    .with_detail(redact_url(url)),
                ))
```

`reqwest::Error`'s `Display` embeds the full request URL, query included. In `download/mod.rs` and `meta/mod.rs`, every `.with_detail(e)` where `e` is a `reqwest::Error` therefore becomes `.with_detail(e.without_url())`. That covers:
- the builder error
- `HostUnreachable`
- `ConnectionDropped` (both files)
- `Offline`

Split `fetch` into a locking wrapper and the old body:

```rust
    pub async fn fetch(&self, req: &DownloadRequest) -> Result<()> {
        let key = lock_key(&req.dest);
        let dest_lock = {
            let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
            locks.entry(key.clone()).or_default().clone()
        };
        let result = {
            let _guard = dest_lock.clone().lock_owned().await;
            self.fetch_locked(req).await
        };
        // Forget the lock once nobody else holds or waits for it. Clones are only handed out
        // under the map's mutex, so the count can't change while it is checked here.
        let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
        if Arc::strong_count(&dest_lock) == 2 {
            locks.remove(&key);
        }
        result
    }

    async fn fetch_locked(&self, req: &DownloadRequest) -> Result<()> {
        if self.already_valid(req).await {
            return Ok(());
        }
        // … the rest of the old `fetch` body, unchanged, from `create_dir_all(parent)` on …
    }
```

In `meta/mod.rs`:
- Delete its private `host_of` and `use crate::download::{host_of, redact_url};`.
- Make the three messages host-only:
  - `format!("Downloading from {} was interrupted", host_of(url))`: keep its reqwest-error detail and add `.with_param("host", host_of(url))`.
  - `format!("{} answered {status}", host_of(url))` plus `.with_detail(redact_url(url))`.
  - `format!("Data from {} didn't match its checksum", host_of(url))` plus `.with_detail(redact_url(url))`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core`
Expected: PASS. If an older test asserted a full URL in a message, change it to assert the host.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/download/mod.rs crates/launcher-core/src/meta/mod.rs crates/launcher-core/tests/download.rs crates/launcher-core/tests/meta.rs
git commit -m "fix(core): name only the host in download and metadata errors; prune destination locks

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Locks: per instance, across-process installs, and the natives marker order

**Files:**
- Create: `crates/launcher-core/src/locks.rs`
- Modify: `crates/launcher-core/src/lib.rs` (`pub mod locks;`)
- Modify: `crates/launcher-core/src/paths.rs` (`objects()`, `install_lock()`, and the layout test)
- Modify: `crates/launcher-core/src/install/mod.rs` (`Installer::install` takes the install lock)
- Modify: `crates/launcher-core/src/install/natives.rs` (`swap_in`)

**Interfaces:**
- Produces:
  - `launcher_core::locks::InstanceLock::try_acquire(perland_dir: &Path) -> Result<InstanceLock>`. It fails at once with `ErrorCode::InstanceLocked` while another holder exists, in this process or another. Dropping it releases the lock.
  - `launcher_core::locks::InstallLock::acquire(path: &Path)` (async) `-> Result<InstallLock>`. It waits for the holder.
  - `LauncherDirs::objects() -> PathBuf` = `<root>/cache/objects`.
  - `LauncherDirs::install_lock() -> PathBuf` = `<root>/.install.lock`.

- [ ] **Step 1: Write the failing tests**

The test module at the bottom of the new `crates/launcher-core/src/locks.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;
    use std::time::Duration;

    #[test]
    fn a_second_instance_lock_is_refused_until_the_first_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let first = InstanceLock::try_acquire(dir.path()).unwrap();
        assert_eq!(
            InstanceLock::try_acquire(dir.path()).unwrap_err().code,
            ErrorCode::InstanceLocked
        );
        drop(first);
        InstanceLock::try_acquire(dir.path()).unwrap();
    }

    #[tokio::test]
    async fn install_lock_waits_for_the_holder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/.install.lock");
        let first = InstallLock::acquire(&path).await.unwrap();
        let p2 = path.clone();
        let waiter = tokio::spawn(async move { InstallLock::acquire(&p2).await });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!waiter.is_finished(), "must wait while the lock is held");
        drop(first);
        tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .expect("acquired once released")
            .unwrap()
            .unwrap();
    }
}
```

In `paths.rs` `layout_is_under_root`, add:

```rust
        assert_eq!(d.objects(), root.join("cache/objects"));
        assert_eq!(d.install_lock(), root.join(".install.lock"));
```

In `natives.rs` `mod tests`:

```rust
    #[test]
    fn a_failed_swap_leaves_no_marker_behind() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("natives");
        let tmp = dir.path().join("natives.tmp");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join(MARKER), "old").unwrap();
        fs::write(dest.join("lwjgl.dll"), "x").unwrap();
        fs::create_dir_all(&tmp).unwrap();
        let err = swap_in(&tmp, &dest, |_| Err(io::Error::other("in use"))).unwrap_err();
        assert_eq!(err.code, ErrorCode::VersionInUse);
        assert!(
            !dest.join(MARKER).exists(),
            "the old marker must be gone before the folder is removed"
        );
        assert!(!tmp.exists());
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib -- locks paths natives`
Expected: compile errors (`locks` is not in `lib.rs` yet; `objects`, `install_lock` and `swap_in` are missing).

- [ ] **Step 3: Implement**

The top of `crates/launcher-core/src/locks.rs`:

```rust
//! OS file locks. One per instance, held while its files change (sync, and from Plan 3b a
//! running game); one for installing shared game files, so the app and the CLI never write them
//! at the same time. The OS releases both when the process exits, so a crash never leaves a
//! stale lock behind.
use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
};

use crate::error::{ErrorCategory, ErrorCode, LauncherError, Result};

fn open_lock_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
}

/// Held while something changes an instance's files. Dropping it releases the lock.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    /// Lock `<perland_dir>/lock`, or fail at once with `instance_locked` while anyone else
    /// (this process or another) holds it.
    pub fn try_acquire(perland_dir: &Path) -> Result<Self> {
        let file = open_lock_file(&perland_dir.join("lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(LauncherError::game(
                ErrorCode::InstanceLocked,
                "Another Perland Launcher window is using this instance",
            )
            .with_fix("Close the other window, or wait for it to finish")),
            Err(TryLockError::Error(e)) => Err(e.into()),
        }
    }
}

/// Held while shared game files (versions, libraries, assets) are installed.
#[derive(Debug)]
pub struct InstallLock {
    _file: File,
}

impl InstallLock {
    /// Wait until the lock at `path` is free, then hold it.
    pub async fn acquire(path: &Path) -> Result<Self> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || -> Result<Self> {
            let file = open_lock_file(&path)?;
            file.lock()?;
            Ok(Self { _file: file })
        })
        .await
        .map_err(|e| {
            LauncherError::new(
                ErrorCategory::Disk,
                ErrorCode::Internal,
                "Couldn't wait for another install to finish",
            )
            .with_detail(e)
        })?
    }
}
```

`lib.rs`: add `pub mod locks;` (alphabetical, after `launch`).

`paths.rs`:

```rust
    /// Content-addressed pack files (see `download::cache`).
    pub fn objects(&self) -> PathBuf {
        self.root.join("cache").join("objects")
    }
    /// Locked while shared game files are installed, across every launcher process.
    pub fn install_lock(&self) -> PathBuf {
        self.root.join(".install.lock")
    }
```

`install/mod.rs`, first line of `Installer::install`:

```rust
        // The app serializes its own installs; this also keeps a CLI run out of the way.
        let _install_lock = crate::locks::InstallLock::acquire(&self.dirs.install_lock()).await?;
```

`natives.rs`: replace everything in `extract_all` from `if dest.exists() {` to the end with `swap_in(&tmp, dest, |d| fs::remove_dir_all(d))`, and add:

```rust
/// Replace `dest` with the freshly built `tmp`. The old marker is deleted first: if removing the
/// old folder then fails halfway (a running game holds a DLL), what's left can never pass for a
/// complete build of any set of jars.
fn swap_in(tmp: &Path, dest: &Path, remove_dir: impl Fn(&Path) -> io::Result<()>) -> Result<()> {
    if dest.exists() {
        let _ = fs::remove_file(dest.join(MARKER));
        if let Err(e) = remove_dir(dest) {
            let _ = fs::remove_dir_all(tmp);
            return Err(LauncherError::game(
                ErrorCode::VersionInUse,
                "Another copy of this Minecraft version is running",
            )
            .with_fix("Close it and try again")
            .with_detail(e));
        }
    }
    fs::rename(tmp, dest)?;
    Ok(())
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/locks.rs crates/launcher-core/src/lib.rs crates/launcher-core/src/paths.rs crates/launcher-core/src/install/mod.rs crates/launcher-core/src/install/natives.rs
git commit -m "feat(core): per-instance and cross-process install locks; drop the natives marker first

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: The object cache, and hash-verified metadata bytes

**Files:**
- Create: `crates/launcher-core/src/download/cache.rs`
- Modify: `crates/launcher-core/src/download/mod.rs` (`pub mod cache;`)
- Modify: `crates/launcher-core/src/meta/mod.rs` (`get_verified_bytes`; `get_verified_json` uses it)
- Test: `crates/launcher-core/tests/cache.rs`

**Interfaces:**
- Consumes: `Hashes::cache_key` (Task 3), `Downloader`/`ExistingCheck::Hash`.
- Produces `launcher_core::download::cache::ObjectCache`:
  - `new(dir: impl Into<PathBuf>)` and `dir(&self) -> &Path`.
  - `path_for(&self, &Hashes) -> Option<PathBuf>`: `<dir>/<first 2 chars of key>/<key>`.
  - `async get(&self, &Hashes) -> Option<PathBuf>`: the path only when the object exists and matches every hash; no network.
  - `async ensure(&self, &Downloader, urls: &[String], &Hashes, size: Option<u64>) -> Result<PathBuf>`: downloads when the object is missing or damaged (`ExistingCheck::Hash`). A `Hashes` with no usable key gives `PackInvalid`.
  - `async insert_file(&self, src: &Path, &Hashes) -> Result<PathBuf>`: verifies, then copies in atomically. A mismatch gives `DownloadCorrupt` and nothing is written.
- Produces `MetaClient::get_verified_bytes(&self, url: &str, hashes: &Hashes) -> Result<Vec<u8>>` (`pub(crate)`):
  - A cached copy that matches `hashes` is returned without any network access.
  - Otherwise it fetches with that validation.
  - The final bytes always match.

- [ ] **Step 1: Write the failing tests** (`crates/launcher-core/tests/cache.rs`)

```rust
use std::time::Duration;

use launcher_core::{
    download::{cache::ObjectCache, hash::sha1_hex, Downloader, Hashes},
    ErrorCode,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn downloader() -> Downloader {
    Downloader::new(reqwest::Client::new()).with_backoff(Duration::from_millis(1))
}

async fn serve(server: &MockServer, at: &str, body: &str, times: u64) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .expect(times)
        .mount(server)
        .await;
}

#[tokio::test]
async fn ensure_downloads_once_then_serves_from_disk() {
    let server = MockServer::start().await;
    serve(&server, "/a.jar", "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(dir.path());
    let hashes = Hashes::sha1(sha1_hex(b"hello"));
    let urls = vec![format!("{}/a.jar", server.uri())];
    let first = cache.ensure(&downloader(), &urls, &hashes, Some(5)).await.unwrap();
    assert_eq!(
        first,
        dir.path().join("aa").join("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d")
    );
    let second = cache.ensure(&downloader(), &urls, &hashes, Some(5)).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "hello");
    assert_eq!(cache.get(&hashes).await, Some(first));
}

#[tokio::test]
async fn a_damaged_object_is_downloaded_again() {
    let server = MockServer::start().await;
    serve(&server, "/a.jar", "hello", 2).await;
    let dir = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(dir.path());
    let hashes = Hashes::sha1(sha1_hex(b"hello"));
    let urls = vec![format!("{}/a.jar", server.uri())];
    let path = cache.ensure(&downloader(), &urls, &hashes, Some(5)).await.unwrap();
    std::fs::write(&path, "jello").unwrap(); // same size, wrong bytes
    assert_eq!(cache.get(&hashes).await, None, "get never returns a damaged object");
    cache.ensure(&downloader(), &urls, &hashes, Some(5)).await.unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
}

#[tokio::test]
async fn insert_file_checks_before_copying() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(dir.path().join("objects"));
    let src = dir.path().join("download.jar");
    std::fs::write(&src, "jello").unwrap();
    let hashes = Hashes::sha1(sha1_hex(b"hello"));
    let err = cache.insert_file(&src, &hashes).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::DownloadCorrupt);
    assert!(!dir.path().join("objects").exists(), "nothing written on a mismatch");
    std::fs::write(&src, "hello").unwrap();
    let stored = cache.insert_file(&src, &hashes).await.unwrap();
    assert_eq!(cache.get(&hashes).await, Some(stored));
    assert!(src.exists(), "the player's file is copied, not moved");
}

#[test]
fn files_without_a_usable_hash_have_no_place() {
    let cache = ObjectCache::new("objects");
    assert_eq!(cache.path_for(&Hashes::default()), None);
    assert_eq!(cache.path_for(&Hashes::sha1("../../x")), None);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --test cache`
Expected: compile error, `could not find cache in download`.

- [ ] **Step 3: Implement** `crates/launcher-core/src/download/cache.rs`

```rust
//! The content-addressed store for pack files: `cache/objects/<2-char prefix>/<key>`, keyed by
//! [`Hashes::cache_key`]. A file is downloaded once for every instance that uses it, and a sync
//! interrupted halfway resumes from here. Objects are copied out, never linked, so a player's
//! edit in one instance can't reach the cache or another instance.
use std::path::{Path, PathBuf};

use super::{DownloadRequest, Downloader, ExistingCheck, Hashes};
use crate::error::{ErrorCode, LauncherError, Result};

#[derive(Debug, Clone)]
pub struct ObjectCache {
    dir: PathBuf,
}

fn no_usable_hash() -> LauncherError {
    LauncherError::pack(ErrorCode::PackInvalid, "A pack file has no usable checksum")
        .with_param("file", "checksum")
}

async fn verify(hashes: &Hashes, path: &Path) -> Result<bool> {
    let (h, p) = (hashes.clone(), path.to_path_buf());
    let ok = tokio::task::spawn_blocking(move || h.verify_file(&p))
        .await
        .map_err(std::io::Error::other)??;
    Ok(ok)
}

impl ObjectCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the object for `hashes` lives, or `None` when they hold no usable hash.
    pub fn path_for(&self, hashes: &Hashes) -> Option<PathBuf> {
        let key = hashes.cache_key()?;
        Some(self.dir.join(&key[..2]).join(&key))
    }

    /// The object, only if it is present and matches every hash. Never touches the network.
    pub async fn get(&self, hashes: &Hashes) -> Option<PathBuf> {
        let path = self.path_for(hashes)?;
        matches!(verify(hashes, &path).await, Ok(true)).then_some(path)
    }

    /// The verified object, downloaded from `urls` when it is missing or damaged.
    pub async fn ensure(
        &self,
        downloader: &Downloader,
        urls: &[String],
        hashes: &Hashes,
        size: Option<u64>,
    ) -> Result<PathBuf> {
        let dest = self.path_for(hashes).ok_or_else(no_usable_hash)?;
        let req = DownloadRequest {
            urls: urls.to_vec(),
            dest: dest.clone(),
            hashes: hashes.clone(),
            size,
        };
        downloader
            .clone()
            .with_existing_check(ExistingCheck::Hash)
            .fetch(&req)
            .await?;
        Ok(dest)
    }

    /// Copy a local file in (a manual download, an imported `.mrpack`) after checking it.
    pub async fn insert_file(&self, src: &Path, hashes: &Hashes) -> Result<PathBuf> {
        let dest = self.path_for(hashes).ok_or_else(no_usable_hash)?;
        if !verify(hashes, src).await? {
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Err(LauncherError::integrity(
                ErrorCode::DownloadCorrupt,
                format!("{name} isn't the file the pack expects"),
            )
            .with_param("file", name));
        }
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut tmp = dest.as_os_str().to_owned();
        tmp.push(format!(".{}.tmp", std::process::id()));
        let tmp = PathBuf::from(tmp);
        let copied = match tokio::fs::copy(src, &tmp).await {
            Ok(_) => tokio::fs::rename(&tmp, &dest).await,
            Err(e) => Err(e),
        };
        if let Err(e) = copied {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(e.into());
        }
        Ok(dest)
    }
}
```

In `download/mod.rs`, add `pub mod cache;` next to `pub mod hash;`.

In `meta/mod.rs`, add the following (importing `crate::download::Hashes`) and rewrite `get_verified_json`'s body as `let bytes = self.get_verified_bytes(url, &Hashes::sha1(sha1)).await?; parse_json(url, &bytes)`:

```rust
    /// Bytes that must match `hashes` (never empty): a matching cached copy is used without any
    /// network access; otherwise the URL is fetched and only matching bytes are cached or returned.
    pub(crate) async fn get_verified_bytes(&self, url: &str, hashes: &Hashes) -> Result<Vec<u8>> {
        if let Ok(cached) = tokio::fs::read(self.body_path(url)).await {
            if hashes.verify(&cached) {
                return Ok(cached);
            }
        }
        let verify = |b: &[u8]| {
            if hashes.verify(b) {
                Ok(())
            } else {
                Err(LauncherError::integrity(
                    ErrorCode::MetaChecksum,
                    format!("Data from {} didn't match its checksum", host_of(url)),
                )
                .with_detail(redact_url(url))
                .with_fix("Try again in a minute"))
            }
        };
        let bytes = self.fetch(url, verify).await?;
        // A cached fallback (offline or server error) bypasses fetch's validation of fresh bodies.
        verify(&bytes)?;
        Ok(bytes)
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core`
Expected: PASS, including the existing `get_verified_json` tests in `tests/meta.rs`.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/download crates/launcher-core/src/meta/mod.rs crates/launcher-core/tests/cache.rs
git commit -m "feat(core): content-addressed object cache and hash-verified metadata bytes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Source types, pack input, CurseForge addresses, the `launcher.toml` sidecar

**Files:**
- Create: `crates/launcher-core/src/sources/mod.rs`, `crates/launcher-core/src/sources/curseforge.rs`, `crates/launcher-core/src/sources/sidecar.rs`
- Modify: `crates/launcher-core/src/lib.rs` (`pub mod sources;`)
- Modify: `Cargo.toml` (workspace deps) and `crates/launcher-core/Cargo.toml`: add `toml = "1"` and `percent-encoding = "2"`

**Interfaces:**
- Produces, in `launcher_core::sources`. Every type derives `Debug, Clone, PartialEq, Serialize, Deserialize` (serde camelCase; enums snake_case):
  - `enum Side { Client, Server, Both }`, with `on_client(self) -> bool`.
  - `enum FileMode { Replace, Preserve, ReplaceUnlessModified }`
  - `enum FileSource { Remote { urls: Vec<String> }, Archive { entry: String } }`, tagged `kind`.
  - `struct OptionalFile { default: bool, description: Option<String> }`
  - `struct ManualDownload { page_url: String, file_name: String }`
  - `struct PackFile { path, name, source, hashes: Hashes, size: Option<u64>, side, optional: Option<OptionalFile>, mode, manual: Option<ManualDownload> }`
  - `struct PackArt { icon: Option<String>, banner: Option<String> }` (absolute URLs) and `struct PackAbout { description: Option<String>, art: PackArt, policy: Option<UpdatePolicy> }`. Both derive `Default`.
  - `struct DesiredState { name, pack_version, minecraft, loader: Option<LoaderSpec>, files: Vec<PackFile>, stamp: String, about: PackAbout, archive: Option<Hashes> }`, with `check_paths(&self) -> Result<()>` (`PackUnsafePath` on a case-insensitive collision).
- Produces, also in `launcher_core::sources`:
  - `enum UpdateCheck { UpToDate, Available { source: SourceRef, stamp: String, version: String }, Unavailable(LauncherError) }`
  - `enum PackInput { Packwiz { url }, MrpackFile { path: PathBuf }, Modrinth { project, version: Option<String> } }`, with `PackInput::parse(&str) -> Result<PackInput>`.
  - `fn check_pack_url(&str) -> Result<reqwest::Url>`: https, or http on a loopback host; anything else is `PackLinkInvalid`.
  - `fn valid_modrinth_id(&str) -> bool`
  - `struct SourceConfig { modrinth_api, curseforge_cdn, curseforge_site }`, with `Default` set to the real addresses.
  - `fn game_from(&BTreeMap<String, String>) -> Result<(String, Option<LoaderSpec>)>`
  - `pub(crate)` error helpers: `unsupported(what)`, `invalid(file, detail)`, `unsafe_path(path)`, `encode_path(rel) -> String` (percent-encodes each segment for URL joins).
- Produces `sources::curseforge::{cdn_url(cdn, file_id, file_name) -> String, page_url(site, project_id) -> String}`.
- Produces `sources::sidecar::parse_sidecar(text: &str, pack_url: &str) -> PackAbout`. It never fails: anything unusable reads as "no sidecar".

- [ ] **Step 1: Write the failing tests**

`crates/launcher-core/src/sources/curseforge.rs`, test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdn_urls_split_the_id_and_escape_the_name() {
        let cdn = "https://mediafilez.forgecdn.net";
        assert_eq!(
            cdn_url(cdn, 8977190, "music_disc_maker-3.0.5+forge-1.20.1.jar"),
            "https://mediafilez.forgecdn.net/files/8977/190/music_disc_maker-3.0.5%2Bforge-1.20.1.jar"
        );
        assert_eq!(
            cdn_url(cdn, 5512063, "a b.jar"),
            "https://mediafilez.forgecdn.net/files/5512/63/a%20b.jar"
        );
        assert_eq!(
            page_url("https://www.curseforge.com", 825733),
            "https://www.curseforge.com/projects/825733"
        );
    }
}
```

`crates/launcher-core/src/sources/sidecar.rs`, test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::UpdatePolicy;

    const PACK: &str = "https://perlanddev.github.io/asalith-modpack/pack.toml";

    #[test]
    fn sidecar_art_resolves_against_the_pack() {
        let about = parse_sidecar(
            "[art]\nicon = \"assets/favicon.svg\"\nbanner = \"https://cdn.example.com/b.png\"\n\
             [about]\ndescription = \"Official Asalith server modpack\"\n[sync]\npolicy = \"auto\"\n\
             [future]\nunknown = 1\n",
            PACK,
        );
        assert_eq!(
            about.art.icon.as_deref(),
            Some("https://perlanddev.github.io/asalith-modpack/assets/favicon.svg")
        );
        assert_eq!(about.art.banner.as_deref(), Some("https://cdn.example.com/b.png"));
        assert_eq!(about.description.as_deref(), Some("Official Asalith server modpack"));
        assert_eq!(about.policy, Some(UpdatePolicy::Auto));
    }

    #[test]
    fn sidecar_refuses_other_schemes_and_ignores_garbage() {
        let about = parse_sidecar(
            "[art]\nicon = \"javascript:alert(1)\"\nbanner = \"file:///C:/x.png\"\n[sync]\npolicy = \"pinned\"\n",
            PACK,
        );
        assert_eq!(about, PackAbout::default());
        let http = parse_sidecar("[art]\nicon = \"http://evil.example/x.png\"\n", PACK);
        assert_eq!(http.art.icon, None, "an https pack never pulls art over plain http");
        assert_eq!(parse_sidecar("not toml [", PACK), PackAbout::default());
    }
}
```

In `crates/launcher-core/src/sources/mod.rs`, the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_input_requires_https_except_loopback() {
        assert_eq!(
            PackInput::parse(" https://perlanddev.github.io/asalith-modpack/pack.toml ").unwrap(),
            PackInput::Packwiz {
                url: "https://perlanddev.github.io/asalith-modpack/pack.toml".into()
            }
        );
        for ok in [
            "http://127.0.0.1:8080/pack.toml",
            "http://localhost/pack.toml",
            "http://[::1]:9/pack.toml",
        ] {
            assert!(matches!(PackInput::parse(ok), Ok(PackInput::Packwiz { .. })), "{ok}");
        }
        for bad in [
            "http://example.com/pack.toml",
            "ftp://example.com/pack.toml",
            "file:///C:/pack.toml",
            "javascript:alert(1)",
            "",
            "pack.toml",
            "modrinth:../x",
            "modrinth:",
        ] {
            assert_eq!(
                PackInput::parse(bad).unwrap_err().code,
                ErrorCode::PackLinkInvalid,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn pack_input_reads_modrinth_and_mrpack() {
        assert_eq!(
            PackInput::parse("modrinth:fabulously-optimized").unwrap(),
            PackInput::Modrinth { project: "fabulously-optimized".into(), version: None }
        );
        assert_eq!(
            PackInput::parse("modrinth:1KVo5zza@2ZbcYfCj").unwrap(),
            PackInput::Modrinth { project: "1KVo5zza".into(), version: Some("2ZbcYfCj".into()) }
        );
        assert_eq!(
            PackInput::parse("C:\\Downloads\\Pack 1.0.MRPACK").unwrap(),
            PackInput::MrpackFile { path: "C:\\Downloads\\Pack 1.0.MRPACK".into() }
        );
    }

    fn versions(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn game_from_reads_loaders() {
        let (mc, loader) = game_from(&versions(&[("minecraft", "1.20.1"), ("forge", "47.4.22")])).unwrap();
        assert_eq!(mc, "1.20.1");
        assert_eq!(loader, Some(LoaderSpec { kind: LoaderKind::Forge, version: "47.4.22".into() }));
        let (_, prefixed) =
            game_from(&versions(&[("minecraft", "1.20.1"), ("forge", "1.20.1-47.4.22")])).unwrap();
        assert_eq!(prefixed.unwrap().version, "47.4.22");
        let (_, fabric) =
            game_from(&versions(&[("minecraft", "1.20.1"), ("fabric-loader", "0.16.14")])).unwrap();
        assert_eq!(fabric.unwrap().kind, LoaderKind::Fabric);
        assert_eq!(game_from(&versions(&[("minecraft", "1.20.1")])).unwrap().1, None);
        let lite = game_from(&versions(&[("minecraft", "1.12.2"), ("liteloader", "1")]));
        assert_eq!(lite.unwrap_err().code, ErrorCode::PackUnsupported);
        let two = game_from(&versions(&[("minecraft", "1.20.1"), ("forge", "1"), ("fabric", "2")]));
        assert_eq!(two.unwrap_err().code, ErrorCode::PackUnsupported);
        assert_eq!(game_from(&versions(&[])).unwrap_err().code, ErrorCode::PackInvalid);
    }

    fn file(path: &str) -> PackFile {
        PackFile {
            path: path.into(),
            name: path.into(),
            source: FileSource::Remote { urls: vec![] },
            hashes: Hashes::sha1("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"),
            size: None,
            side: Side::Both,
            optional: None,
            mode: FileMode::Replace,
            manual: None,
        }
    }

    fn desired(files: Vec<PackFile>) -> DesiredState {
        DesiredState {
            name: "P".into(),
            pack_version: "1".into(),
            minecraft: "1.20.1".into(),
            loader: None,
            files,
            stamp: "s".into(),
            about: PackAbout::default(),
            archive: None,
        }
    }

    #[test]
    fn check_paths_refuses_case_collisions() {
        desired(vec![file("mods/a.jar"), file("mods/b.jar")]).check_paths().unwrap();
        let clash = desired(vec![file("mods/a.jar"), file("MODS/A.jar")]);
        assert_eq!(clash.check_paths().unwrap_err().code, ErrorCode::PackUnsafePath);
    }

    #[test]
    fn encode_path_escapes_url_syntax_per_segment() {
        assert_eq!(encode_path("config/a b#1?.toml"), "config/a%20b%231%3F.toml");
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib sources`
Expected: compile error, `sources` is not a module yet.

- [ ] **Step 3: Implement**

`Cargo.toml` `[workspace.dependencies]`: add `toml = "1"` and `percent-encoding = "2"`. In `crates/launcher-core/Cargo.toml` `[dependencies]`: add `toml.workspace = true` and `percent-encoding.workspace = true`.

`lib.rs`: add `pub mod sources;`.

`crates/launcher-core/src/sources/mod.rs` (the tests from Step 1 go at the bottom):

```rust
//! Where packs come from. Every source (packwiz, `.mrpack`, Modrinth) reduces to a
//! [`DesiredState`]; `sync` never knows which one it was (core spec §2.1).
pub mod curseforge;
pub mod sidecar;

use std::{collections::BTreeMap, fmt::Display, path::PathBuf};

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::{Deserialize, Serialize};

use crate::{
    download::Hashes,
    error::{ErrorCode, LauncherError, Result},
    fsutil::path_key,
    instances::{LoaderSpec, SourceRef, UpdatePolicy},
    meta::loaders::LoaderKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Client,
    Server,
    Both,
}

impl Side {
    /// Only client and both-side files are installed (spec §3.2).
    pub fn on_client(self) -> bool {
        self != Side::Server
    }
}

/// What sync may do with a file that differs on disk (spec §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileMode {
    /// The pack wins.
    Replace,
    /// packwiz `preserve`: written only when the file is absent.
    Preserve,
    /// `.mrpack` overrides outside `mods/`: replaced only while the player hasn't changed it.
    ReplaceUnlessModified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileSource {
    Remote { urls: Vec<String> },
    /// An entry inside the pack's cached `.mrpack` (see [`DesiredState::archive`]).
    Archive { entry: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionalFile {
    pub default: bool,
    pub description: Option<String>,
}

/// Where a player can download a file by hand when its author blocks other launchers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualDownload {
    pub page_url: String,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackFile {
    /// Canonical relative path inside the game folder (`fsutil::normalize_rel`).
    pub path: String,
    /// What changelogs call it: the mod's name, or the file name.
    pub name: String,
    pub source: FileSource,
    /// Never empty: every source refuses files without a usable checksum.
    pub hashes: Hashes,
    pub size: Option<u64>,
    pub side: Side,
    pub optional: Option<OptionalFile>,
    pub mode: FileMode,
    pub manual: Option<ManualDownload>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackArt {
    pub icon: Option<String>,
    pub banner: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackAbout {
    pub description: Option<String>,
    pub art: PackArt,
    /// The update policy a pack asks for (`launcher.toml` `[sync] policy`); only the default.
    pub policy: Option<UpdatePolicy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesiredState {
    pub name: String,
    pub pack_version: String,
    pub minecraft: String,
    pub loader: Option<LoaderSpec>,
    pub files: Vec<PackFile>,
    /// What the cheap update check compares: packwiz `pack.toml`'s sha256, a Modrinth version
    /// id, or a local `.mrpack`'s sha1.
    pub stamp: String,
    pub about: PackAbout,
    /// The `.mrpack` that `Archive` entries come from, by its hashes in the object cache.
    pub archive: Option<Hashes>,
}

impl DesiredState {
    /// Refuse packs where two files would land on one path on a case-insensitive disk.
    pub fn check_paths(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for f in &self.files {
            if !seen.insert(path_key(&f.path)) {
                return Err(unsafe_path(&f.path));
            }
        }
        Ok(())
    }
}

/// The result of the cheap update check (spec §3.1).
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateCheck {
    UpToDate,
    /// A newer version. `source` is what to resolve; for Modrinth it names the new version.
    Available {
        source: SourceRef,
        stamp: String,
        version: String,
    },
    /// The check itself failed (offline, server trouble): play what's installed.
    Unavailable(LauncherError),
}

/// What a player hands the launcher to make a pack instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackInput {
    Packwiz { url: String },
    MrpackFile { path: PathBuf },
    Modrinth { project: String, version: Option<String> },
}

fn link_invalid() -> LauncherError {
    LauncherError::pack(ErrorCode::PackLinkInvalid, "That doesn't look like a pack link")
        .with_fix("Pack links start with https:// and usually end in pack.toml")
}

impl PackInput {
    /// `https://…/pack.toml`, `modrinth:<slug or id>[@<version id>]`, or a path ending in `.mrpack`.
    pub fn parse(input: &str) -> Result<PackInput> {
        let s = input.trim();
        if let Some(rest) = s.strip_prefix("modrinth:") {
            let (project, version) = match rest.split_once('@') {
                Some((p, v)) => (p, Some(v)),
                None => (rest, None),
            };
            if !valid_modrinth_id(project) || version.is_some_and(|v| !valid_modrinth_id(v)) {
                return Err(link_invalid());
            }
            return Ok(PackInput::Modrinth {
                project: project.to_string(),
                version: version.map(str::to_string),
            });
        }
        if s.starts_with("http://") || s.starts_with("https://") {
            check_pack_url(s)?;
            return Ok(PackInput::Packwiz { url: s.to_string() });
        }
        if s.to_ascii_lowercase().ends_with(".mrpack") {
            return Ok(PackInput::MrpackFile { path: PathBuf::from(s) });
        }
        Err(link_invalid())
    }
}

/// A pack link the launcher will fetch: `https://`, or `http://` on this computer (tests and
/// local pack development). Mods are code, so a pack is never fetched over plain HTTP from
/// the network.
pub fn check_pack_url(raw: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| link_invalid())?;
    let host = url.host_str().unwrap_or("");
    let loopback = host == "localhost"
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    match url.scheme() {
        "https" => Ok(url),
        "http" if loopback => Ok(url),
        _ => Err(link_invalid()),
    }
}

/// Modrinth slugs and ids go into API paths, so only their documented characters pass.
pub fn valid_modrinth_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Upstream addresses, overridable for tests.
#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub modrinth_api: String,
    pub curseforge_cdn: String,
    pub curseforge_site: String,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            modrinth_api: "https://api.modrinth.com/v2".into(),
            curseforge_cdn: "https://mediafilez.forgecdn.net".into(),
            curseforge_site: "https://www.curseforge.com".into(),
        }
    }
}

/// Minecraft and the loader from a pack's version table (packwiz `[versions]`, `.mrpack`
/// `dependencies`). A `<minecraft>-` prefix on the loader version is dropped.
pub fn game_from(versions: &BTreeMap<String, String>) -> Result<(String, Option<LoaderSpec>)> {
    let minecraft = versions
        .get("minecraft")
        .ok_or_else(|| invalid("versions", "no minecraft version"))?
        .clone();
    let mut loader = None;
    for (key, version) in versions {
        let kind = match key.as_str() {
            "minecraft" => continue,
            "forge" => LoaderKind::Forge,
            "neoforge" => LoaderKind::NeoForge,
            "fabric" | "fabric-loader" => LoaderKind::Fabric,
            "quilt" | "quilt-loader" => LoaderKind::Quilt,
            other => return Err(unsupported(&format!("the {other} loader"))),
        };
        if loader.is_some() {
            return Err(unsupported("more than one loader"));
        }
        let version = version
            .strip_prefix(&format!("{minecraft}-"))
            .unwrap_or(version)
            .to_string();
        loader = Some(LoaderSpec { kind, version });
    }
    Ok((minecraft, loader))
}

pub(crate) fn unsupported(what: &str) -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackUnsupported,
        format!("This pack uses something the launcher can't handle yet: {what}"),
    )
    .with_param("what", what)
    .with_fix("Ask the pack's author, or try another pack")
}

pub(crate) fn invalid(file: &str, detail: impl Display) -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackInvalid,
        format!("The pack file {file} is damaged or isn't a pack"),
    )
    .with_param("file", file)
    .with_fix("Check the link, or ask the pack's author")
    .with_detail(detail)
}

pub(crate) fn unsafe_path(path: &str) -> LauncherError {
    let shown: String = path.chars().take(120).collect();
    LauncherError::pack(
        ErrorCode::PackUnsafePath,
        format!("The pack tried to write outside its folder ({shown})"),
    )
    .with_param("path", shown)
    .with_fix("Don't use this pack, and let its author know")
}

/// Characters escaped in each path segment before it is joined onto a URL.
const SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// A canonical relative path, percent-encoded segment by segment for `Url::join`.
pub(crate) fn encode_path(rel: &str) -> String {
    rel.split('/')
        .map(|s| utf8_percent_encode(s, SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/")
}
```

`crates/launcher-core/src/sources/curseforge.rs`:

```rust
//! CurseForge files referenced by packwiz packs (spec §3.4). Only the public CDN is used, and
//! there is no API key. Addresses checked against real files on 2026-10-10:
//! `edge.forgecdn.net` redirects here and answers range requests with 404, and a raw `+` in a
//! file name gets 403, so names are fully percent-encoded.
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

/// Bytes a CDN file name keeps as they are; everything else is escaped.
const FILE_NAME: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub fn cdn_url(cdn: &str, file_id: u64, file_name: &str) -> String {
    format!(
        "{}/files/{}/{}/{}",
        cdn.trim_end_matches('/'),
        file_id / 1000,
        file_id % 1000,
        utf8_percent_encode(file_name, FILE_NAME)
    )
}

/// The project's page, where the player can download a file whose author blocks launchers.
pub fn page_url(site: &str, project_id: u64) -> String {
    format!("{}/projects/{project_id}", site.trim_end_matches('/'))
}
```

`crates/launcher-core/src/sources/sidecar.rs`:

```rust
//! `launcher.toml`, the optional file next to `pack.toml` that gives a packwiz pack art, a
//! description and a default update policy (spec §3.5, addendum §3.1).
use serde::Deserialize;

use super::{PackAbout, PackArt};
use crate::instances::UpdatePolicy;

#[derive(Deserialize, Default)]
#[serde(default)]
struct Raw {
    art: RawArt,
    about: RawAbout,
    sync: RawSync,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawArt {
    icon: Option<String>,
    banner: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAbout {
    description: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSync {
    policy: Option<String>,
}

/// What `launcher.toml` says. It never fails: an unreadable file is the same as no file, and
/// the launcher falls back to its generated art.
pub fn parse_sidecar(text: &str, pack_url: &str) -> PackAbout {
    let Ok(raw) = toml::from_str::<Raw>(text) else {
        return PackAbout::default();
    };
    let base = reqwest::Url::parse(pack_url).ok();
    let art = |value: Option<String>| value.and_then(|v| resolve_art(base.as_ref()?, &v));
    let description = raw
        .about
        .description
        .map(|d| d.trim().chars().take(500).collect::<String>())
        .filter(|d| !d.is_empty());
    let policy = match raw.sync.policy.as_deref() {
        Some("auto") => Some(UpdatePolicy::Auto),
        Some("ask") => Some(UpdatePolicy::Ask),
        _ => None,
    };
    PackAbout {
        description,
        art: PackArt {
            icon: art(raw.art.icon),
            banner: art(raw.art.banner),
        },
        policy,
    }
}

/// An art reference as an absolute URL: relative paths resolve against `pack.toml`. Only https,
/// or plain http on the pack's own (local) host.
fn resolve_art(base: &reqwest::Url, value: &str) -> Option<String> {
    let url = match reqwest::Url::parse(value) {
        Ok(abs) => abs,
        Err(_) => base.join(value).ok()?,
    };
    let same_host = url.host_str() == base.host_str();
    match url.scheme() {
        "https" => Some(url.to_string()),
        "http" if base.scheme() == "http" && same_host => Some(url.to_string()),
        _ => None,
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --lib sources`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/launcher-core/Cargo.toml crates/launcher-core/src/lib.rs crates/launcher-core/src/sources
git commit -m "feat(core): pack source types, pack links, CurseForge addresses, launcher.toml sidecar

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: packwiz parsing (pure)

These functions turn bytes into checked `PackFile`s with no I/O, so they are unit-tested here and fuzzed in CI (Task 20).

**Files:**
- Create: `crates/launcher-core/src/sources/packwiz.rs`
- Modify: `crates/launcher-core/src/sources/mod.rs` (`pub mod packwiz;`)

**Interfaces:**
- Consumes:
  - `Hashes::is_known_format`/`from_format` (Task 3)
  - `normalize_rel`/`safe_component` (Task 2)
  - `curseforge::{cdn_url, page_url}`, `unsupported`/`invalid`/`unsafe_path`/`encode_path`, `SourceConfig` (Task 7)
- Produces, in `launcher_core::sources::packwiz`:
  - `PackToml { name, author: Option<String>, version: Option<String>, pack_format: Option<String>, index: IndexRef { file, hash_format, hash }, versions: BTreeMap<String, String> }`
  - `IndexToml { hash_format, files: Vec<IndexEntry { file, hash, hash_format: Option<String>, alias: Option<String>, metafile: bool, preserve: bool }> }`
  - `MetaToml { name, filename, side: Option<String>, download: MetaDownload { url: Option<String>, hash_format, hash, mode: Option<String> }, update: Option<MetaUpdate { curseforge: Option<CfUpdate { file_id: u64, project_id: u64 }> }>, option: Option<MetaOption { optional, default, description }> }`
  - The parsers: `parse_pack(&[u8]) -> Result<PackToml>` (checks `pack-format` is `packwiz:1.x` when present), `parse_index(&[u8]) -> Result<IndexToml>`, and `parse_meta(&[u8], file: &str) -> Result<MetaToml>`.
  - `entry_hashes(format, value, file) -> Result<Hashes>`: an unknown format is `PackUnsupported`, and a malformed value is `PackInvalid`.
  - `meta_file(cfg: &SourceConfig, meta_path: &str, meta: &MetaToml, preserve: bool) -> Result<PackFile>`: the file a metafile installs, next to the metafile.
  - `plain_file(index_url: &reqwest::Url, entry: &IndexEntry, default_format: &str) -> Result<PackFile>`.

- [ ] **Step 1: Write the failing tests** (the bottom of `packwiz.rs`; the samples are the real Asalith Fields files)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sources::{FileMode, FileSource, Side}, ErrorCode};

    const PACK: &str = r#"name = "Asalith Fields"
author = "PerlandDev"
version = "1.0.3"
pack-format = "packwiz:1.1.0"

[index]
file = "index.toml"
hash-format = "sha256"
hash = "30a9c85d91a505ab9c8c6ca9380f705dcc91f74c80e58e16a3d5e244f58226ee"

[versions]
forge = "47.4.22"
minecraft = "1.20.1"

[options]
acceptable-game-versions = ["1.20.1"]
"#;

    const APPLESKIN: &str = r#"name = "AppleSkin"
filename = "appleskin-forge-mc1.20.1-2.5.1.jar"
side = "both"

[download]
url = "https://cdn.modrinth.com/data/EsAfCjCV/versions/XdXDExVF/appleskin-forge-mc1.20.1-2.5.1.jar"
hash-format = "sha512"
hash = "3176f928fb6c60d27f5edb610ca4ab0236175404eebf32797a1ec6cdd794bb294a260231579578536d03571377e43c6181adc75d8dd2c0733f17fce4f7e636b9"

[update]
[update.modrinth]
mod-id = "EsAfCjCV"
version = "XdXDExVF"
"#;

    const COZY: &str = r#"name = "Lucky's Cozy Home: Stone & Waterworks Update"
filename = "cozy_home-3.0.4-forge-1.20.1.jar"
side = "both"

[download]
hash-format = "sha1"
hash = "9b5c0143e9dca96ddcf999f94c8aa0a30495f49f"
mode = "metadata:curseforge"

[update]
[update.curseforge]
file-id = 5512563
project-id = 825733
"#;

    fn cfg() -> SourceConfig {
        SourceConfig::default()
    }

    #[test]
    fn reads_the_asalith_pack_toml() {
        let pack = parse_pack(PACK.as_bytes()).unwrap();
        assert_eq!(pack.name, "Asalith Fields");
        assert_eq!(pack.version.as_deref(), Some("1.0.3"));
        assert_eq!(pack.index.file, "index.toml");
        assert_eq!(pack.index.hash_format, "sha256");
        assert_eq!(pack.versions["forge"], "47.4.22");
    }

    #[test]
    fn pack_format_and_syntax_are_checked() {
        let future = PACK.replace("packwiz:1.1.0", "packwiz:2.0.0");
        assert_eq!(parse_pack(future.as_bytes()).unwrap_err().code, ErrorCode::PackUnsupported);
        let err = parse_pack(b"name = [").unwrap_err();
        assert_eq!(err.code, ErrorCode::PackInvalid);
        assert_eq!(err.params["file"], "pack.toml");
        assert_eq!(parse_pack(&[0xff, 0xfe]).unwrap_err().code, ErrorCode::PackInvalid);
    }

    #[test]
    fn index_entries_keep_their_flags() {
        let index = parse_index(
            b"hash-format = \"sha256\"\n\n[[files]]\nfile = \"mods/a.pw.toml\"\nhash = \"00\"\nmetafile = true\n\n\
              [[files]]\nfile = \"config/x.toml\"\nhash = \"11\"\nhash-format = \"sha1\"\npreserve = true\nalias = \"config/y.toml\"\n",
        )
        .unwrap();
        assert_eq!(index.hash_format, "sha256");
        assert!(index.files[0].metafile && !index.files[0].preserve);
        assert!(index.files[1].preserve);
        assert_eq!(index.files[1].hash_format.as_deref(), Some("sha1"));
        assert_eq!(index.files[1].alias.as_deref(), Some("config/y.toml"));
    }

    #[test]
    fn a_modrinth_metafile_installs_next_to_itself() {
        let meta = parse_meta(APPLESKIN.as_bytes(), "mods/appleskin.pw.toml").unwrap();
        let f = meta_file(&cfg(), "mods/appleskin.pw.toml", &meta, false).unwrap();
        assert_eq!(f.path, "mods/appleskin-forge-mc1.20.1-2.5.1.jar");
        assert_eq!(f.name, "AppleSkin");
        assert_eq!(
            f.source,
            FileSource::Remote {
                urls: vec!["https://cdn.modrinth.com/data/EsAfCjCV/versions/XdXDExVF/appleskin-forge-mc1.20.1-2.5.1.jar".into()]
            }
        );
        assert!(f.hashes.sha512.is_some());
        assert_eq!((f.side, f.mode, f.optional.clone(), f.manual.clone()), (Side::Both, FileMode::Replace, None, None));
    }

    #[test]
    fn a_curseforge_metafile_uses_the_cdn_and_names_its_page() {
        let meta = parse_meta(COZY.as_bytes(), "mods/cozy-home.pw.toml").unwrap();
        let f = meta_file(&cfg(), "mods/cozy-home.pw.toml", &meta, false).unwrap();
        assert_eq!(
            f.source,
            FileSource::Remote {
                urls: vec!["https://mediafilez.forgecdn.net/files/5512/563/cozy_home-3.0.4-forge-1.20.1.jar".into()]
            }
        );
        let manual = f.manual.unwrap();
        assert_eq!(manual.page_url, "https://www.curseforge.com/projects/825733");
        assert_eq!(manual.file_name, "cozy_home-3.0.4-forge-1.20.1.jar");
        assert_eq!(f.hashes.sha1.as_deref(), Some("9b5c0143e9dca96ddcf999f94c8aa0a30495f49f"));
    }

    #[test]
    fn metafile_sides_options_and_preserve() {
        let text = APPLESKIN.replace("side = \"both\"", "side = \"client\"")
            + "\n[option]\noptional = true\ndefault = false\ndescription = \"Shows food values\"\n";
        let meta = parse_meta(text.as_bytes(), "mods/a.pw.toml").unwrap();
        let f = meta_file(&cfg(), "mods/a.pw.toml", &meta, true).unwrap();
        assert_eq!(f.side, Side::Client);
        assert_eq!(f.mode, FileMode::Preserve);
        let opt = f.optional.unwrap();
        assert!(!opt.default);
        assert_eq!(opt.description.as_deref(), Some("Shows food values"));
        let server = APPLESKIN.replace("side = \"both\"", "side = \"server\"");
        let meta = parse_meta(server.as_bytes(), "mods/a.pw.toml").unwrap();
        assert_eq!(meta_file(&cfg(), "mods/a.pw.toml", &meta, false).unwrap().side, Side::Server);
    }

    #[test]
    fn hostile_metafiles_are_refused() {
        for name in ["../evil.jar", "CON.jar", "a/b.jar", "x.jar:stream"] {
            let text = APPLESKIN.replace("appleskin-forge-mc1.20.1-2.5.1.jar\"\nside", &format!("{name}\"\nside"));
            let meta = parse_meta(text.as_bytes(), "mods/a.pw.toml").unwrap();
            assert_eq!(
                meta_file(&cfg(), "mods/a.pw.toml", &meta, false).unwrap_err().code,
                ErrorCode::PackUnsafePath,
                "{name}"
            );
        }
        let mode = COZY.replace("metadata:curseforge", "metadata:ftb");
        let meta = parse_meta(mode.as_bytes(), "mods/c.pw.toml").unwrap();
        assert_eq!(meta_file(&cfg(), "mods/c.pw.toml", &meta, false).unwrap_err().code, ErrorCode::PackUnsupported);
        let crc = APPLESKIN.replace("hash-format = \"sha512\"", "hash-format = \"crc32\"");
        let meta = parse_meta(crc.as_bytes(), "mods/a.pw.toml").unwrap();
        assert_eq!(meta_file(&cfg(), "mods/a.pw.toml", &meta, false).unwrap_err().code, ErrorCode::PackUnsupported);
        let bad = APPLESKIN.replace("hash = \"3176", "hash = \"zz76");
        let meta = parse_meta(bad.as_bytes(), "mods/a.pw.toml").unwrap();
        assert_eq!(meta_file(&cfg(), "mods/a.pw.toml", &meta, false).unwrap_err().code, ErrorCode::PackInvalid);
        let ftp = APPLESKIN.replace("url = \"https://", "url = \"ftp://");
        let meta = parse_meta(ftp.as_bytes(), "mods/a.pw.toml").unwrap();
        assert_eq!(meta_file(&cfg(), "mods/a.pw.toml", &meta, false).unwrap_err().code, ErrorCode::PackUnsupported);
    }

    #[test]
    fn plain_files_resolve_next_to_the_index() {
        let index_url = reqwest::Url::parse("https://example.com/pack/index.toml").unwrap();
        let entry = IndexEntry {
            file: "config/a b#1.toml".into(),
            hash: crate::download::hash::sha1_hex(b"x"),
            hash_format: Some("sha1".into()),
            alias: None,
            metafile: false,
            preserve: true,
        };
        let f = plain_file(&index_url, &entry, "sha256").unwrap();
        assert_eq!(f.path, "config/a b#1.toml");
        assert_eq!(f.name, "a b#1.toml");
        assert_eq!(f.mode, FileMode::Preserve);
        assert_eq!(
            f.source,
            FileSource::Remote { urls: vec!["https://example.com/pack/config/a%20b%231.toml".into()] }
        );
        let aliased = IndexEntry { alias: Some("config/real.toml".into()), ..entry.clone() };
        assert_eq!(plain_file(&index_url, &aliased, "sha256").unwrap().path, "config/real.toml");
        let escaping = IndexEntry { file: "../../etc/passwd".into(), ..entry };
        assert_eq!(plain_file(&index_url, &escaping, "sha256").unwrap_err().code, ErrorCode::PackUnsafePath);
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib sources::packwiz`
Expected: compile error, no module `packwiz`.

- [ ] **Step 3: Implement** `crates/launcher-core/src/sources/packwiz.rs` (and `pub mod packwiz;` in `sources/mod.rs`)

```rust
//! packwiz packs: `pack.toml` → `index.toml` → one `.pw.toml` metafile per mod. Written from
//! packwiz's published format. Everything here is pure; fetching is in `resolve` (Task 9).
use std::collections::BTreeMap;

use serde::{de::DeserializeOwned, Deserialize};

use super::{
    check_pack_url, curseforge, encode_path, invalid, unsafe_path, unsupported, FileMode,
    FileSource, ManualDownload, OptionalFile, PackFile, Side, SourceConfig,
};
use crate::{
    download::Hashes,
    error::Result,
    fsutil::{normalize_rel, safe_component},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PackToml {
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub pack_format: Option<String>,
    pub index: IndexRef,
    #[serde(default)]
    pub versions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IndexRef {
    pub file: String,
    pub hash_format: String,
    pub hash: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IndexToml {
    pub hash_format: String,
    #[serde(default)]
    pub files: Vec<IndexEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IndexEntry {
    pub file: String,
    pub hash: String,
    #[serde(default)]
    pub hash_format: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub metafile: bool,
    #[serde(default)]
    pub preserve: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MetaToml {
    pub name: String,
    pub filename: String,
    #[serde(default)]
    pub side: Option<String>,
    pub download: MetaDownload,
    #[serde(default)]
    pub update: Option<MetaUpdate>,
    #[serde(default)]
    pub option: Option<MetaOption>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MetaDownload {
    #[serde(default)]
    pub url: Option<String>,
    pub hash_format: String,
    pub hash: String,
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetaUpdate {
    #[serde(default)]
    pub curseforge: Option<CfUpdate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CfUpdate {
    pub file_id: u64,
    pub project_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetaOption {
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub description: Option<String>,
}

fn toml_from<T: DeserializeOwned>(bytes: &[u8], file: &str) -> Result<T> {
    let text = std::str::from_utf8(bytes).map_err(|e| invalid(file, e))?;
    toml::from_str(text).map_err(|e| invalid(file, e))
}

pub fn parse_pack(bytes: &[u8]) -> Result<PackToml> {
    let pack: PackToml = toml_from(bytes, "pack.toml")?;
    if let Some(format) = &pack.pack_format {
        if !format.starts_with("packwiz:1.") {
            return Err(unsupported(&format!("pack format {format}")));
        }
    }
    Ok(pack)
}

pub fn parse_index(bytes: &[u8]) -> Result<IndexToml> {
    toml_from(bytes, "index.toml")
}

pub fn parse_meta(bytes: &[u8], file: &str) -> Result<MetaToml> {
    toml_from(bytes, file)
}

/// A pack's hash, checked: an unknown format can't be verified, and a malformed value is damage.
pub fn entry_hashes(format: &str, value: &str, file: &str) -> Result<Hashes> {
    if !Hashes::is_known_format(format) {
        return Err(unsupported(&format!("{format} checksums")));
    }
    Hashes::from_format(format, value).ok_or_else(|| invalid(file, "malformed checksum"))
}

fn side(value: Option<&str>) -> Side {
    match value {
        Some("client") => Side::Client,
        Some("server") => Side::Server,
        _ => Side::Both,
    }
}

/// Pack files download over https (plain http only from this computer, for tests), like packs.
fn check_file_url(url: &str, file: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| invalid(file, e))?;
    if check_pack_url(url).is_ok() {
        Ok(parsed.to_string())
    } else {
        Err(unsupported(&format!("{} downloads", parsed.scheme())))
    }
}

/// The file a metafile installs: `filename` in the metafile's own folder.
pub fn meta_file(
    cfg: &SourceConfig,
    meta_path: &str,
    meta: &MetaToml,
    preserve: bool,
) -> Result<PackFile> {
    if !safe_component(&meta.filename) {
        return Err(unsafe_path(&meta.filename));
    }
    let joined = match meta_path.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{}", meta.filename),
        None => meta.filename.clone(),
    };
    let path = normalize_rel(&joined).ok_or_else(|| unsafe_path(&joined))?;
    let hashes = entry_hashes(&meta.download.hash_format, &meta.download.hash, meta_path)?;
    let (urls, manual) = match meta.download.mode.as_deref().unwrap_or("url") {
        "" | "url" => {
            let url = meta
                .download
                .url
                .as_deref()
                .ok_or_else(|| invalid(meta_path, "no download url"))?;
            (vec![check_file_url(url, meta_path)?], None)
        }
        "metadata:curseforge" => {
            let cf = meta
                .update
                .as_ref()
                .and_then(|u| u.curseforge.as_ref())
                .ok_or_else(|| invalid(meta_path, "no [update.curseforge]"))?;
            (
                vec![curseforge::cdn_url(&cfg.curseforge_cdn, cf.file_id, &meta.filename)],
                Some(ManualDownload {
                    page_url: curseforge::page_url(&cfg.curseforge_site, cf.project_id),
                    file_name: meta.filename.clone(),
                }),
            )
        }
        other => return Err(unsupported(&format!("download mode {other}"))),
    };
    Ok(PackFile {
        path,
        name: meta.name.clone(),
        source: FileSource::Remote { urls },
        hashes,
        size: None,
        side: side(meta.side.as_deref()),
        optional: meta
            .option
            .as_ref()
            .filter(|o| o.optional)
            .map(|o| OptionalFile {
                default: o.default,
                description: o.description.clone(),
            }),
        mode: if preserve { FileMode::Preserve } else { FileMode::Replace },
        manual,
    })
}

/// A file served next to `index.toml` (configs, scripts), installed at its `alias` if it has one.
pub fn plain_file(
    index_url: &reqwest::Url,
    entry: &IndexEntry,
    default_format: &str,
) -> Result<PackFile> {
    let file = normalize_rel(&entry.file).ok_or_else(|| unsafe_path(&entry.file))?;
    let target = match &entry.alias {
        Some(alias) => normalize_rel(alias).ok_or_else(|| unsafe_path(alias))?,
        None => file.clone(),
    };
    let url = index_url
        .join(&encode_path(&file))
        .map_err(|e| invalid(&file, e))?;
    let format = entry.hash_format.as_deref().unwrap_or(default_format);
    let hashes = entry_hashes(format, &entry.hash, &file)?;
    let name = target.rsplit('/').next().unwrap_or(&target).to_string();
    Ok(PackFile {
        path: target,
        name,
        source: FileSource::Remote { urls: vec![url.to_string()] },
        hashes,
        size: None,
        side: Side::Both,
        optional: None,
        mode: if entry.preserve { FileMode::Preserve } else { FileMode::Replace },
        manual: None,
    })
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --lib sources::packwiz`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/sources
git commit -m "feat(core): parse packwiz pack, index and metafiles into checked pack files

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Resolving and checking a packwiz pack over HTTP

**Files:**
- Modify: `crates/launcher-core/src/sources/mod.rs` (the `Sources` struct)
- Modify: `crates/launcher-core/src/sources/packwiz.rs` (`resolve`, `check`)
- Create: `crates/launcher-core/tests/common/mod.rs` (shared by every pack test file)
- Test: `crates/launcher-core/tests/packwiz.rs`

**Interfaces:**
- Consumes:
  - `MetaClient::get_bytes_validated`/`get_bytes` (existing) and `get_verified_bytes` (Task 6)
  - the parsers (Task 8)
  - `parse_sidecar`, `game_from`, `check_pack_url` (Task 7)
- Produces `launcher_core::sources::Sources`. It derives `Clone`, and its fields are public:
  - `http: reqwest::Client`
  - `meta: MetaClient`
  - `downloader: Downloader`
  - `cache: ObjectCache`
  - `config: SourceConfig`
- Produces `packwiz::resolve(src: &Sources, pack_url: &str) -> Result<DesiredState>`:
  - It goes through `MetaClient`, so it works offline from the cache.
  - The stamp is the sha256 of `pack.toml`.
  - A 404 on `pack.toml` with no cached copy gives `PackNotFound`.
- Produces `packwiz::check(src: &Sources, pack_url: &str, installed_stamp: &str) -> UpdateCheck`. It always asks the server; it never uses the cache.
- Produces, in `tests/common/mod.rs`:
  - `sources(server, root) -> Sources`
  - `PackBuilder` (`new`, `vanilla`, `file`, `url_mod`, `cf_mod`, `sidecar`, `corrupt_index`, `pack_toml`, `mount`)
  - `hits(server, path) -> usize`
  - `write_zip(path, entries)`

- [ ] **Step 1: Write the shared test helper** `crates/launcher-core/tests/common/mod.rs`

```rust
//! Shared by the pack integration tests: a tiny packwiz pack served by wiremock, the sources
//! wired to that server, and a zip writer for `.mrpack` files.
#![allow(dead_code)]

use std::{io::Write, path::Path, time::Duration};

use launcher_core::{
    download::{
        cache::ObjectCache,
        hash::{sha1_hex, sha256_hex, sha512_hex},
        Downloader,
    },
    meta::MetaClient,
    sources::{SourceConfig, Sources},
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

pub fn sources(server: &MockServer, root: &Path) -> Sources {
    let http = launcher_core::http::client("test").unwrap();
    Sources {
        http: http.clone(),
        meta: MetaClient::new(http.clone(), root.join("meta")),
        downloader: Downloader::new(http).with_backoff(Duration::from_millis(1)),
        cache: ObjectCache::new(root.join("cache/objects")),
        config: SourceConfig {
            modrinth_api: format!("{}/v2", server.uri()),
            curseforge_cdn: server.uri(),
            curseforge_site: "https://www.curseforge.com".into(),
        },
    }
}

/// How many requests the server saw for `at`.
pub async fn hits(server: &MockServer, at: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == at)
        .count()
}

/// A packwiz pack under `/pack/`, with mod jars under `/files/`.
#[derive(Clone)]
pub struct PackBuilder {
    base: String,
    name: String,
    version: String,
    loader: Option<(String, String)>,
    index: String,
    hosted: Vec<(String, Vec<u8>)>,
    corrupt_index: bool,
}

impl PackBuilder {
    /// Minecraft 1.20.1 with Forge 47.4.22, like Asalith Fields.
    pub fn new(server: &MockServer, name: &str, version: &str) -> Self {
        Self {
            base: server.uri(),
            name: name.into(),
            version: version.into(),
            loader: Some(("forge".into(), "47.4.22".into())),
            index: "hash-format = \"sha256\"\n".into(),
            hosted: Vec::new(),
            corrupt_index: false,
        }
    }

    pub fn vanilla(mut self) -> Self {
        self.loader = None;
        self
    }

    pub fn with_version(mut self, version: &str) -> Self {
        self.version = version.into();
        self
    }

    pub fn with_loader(mut self, kind: &str, version: &str) -> Self {
        self.loader = Some((kind.into(), version.into()));
        self
    }

    fn entry(&mut self, file: &str, body: &[u8], metafile: bool, preserve: bool) {
        self.index += &format!(
            "\n[[files]]\nfile = \"{file}\"\nhash = \"{}\"\nmetafile = {metafile}\npreserve = {preserve}\n",
            sha256_hex(body)
        );
        self.hosted.push((format!("/pack/{file}"), body.to_vec()));
    }

    /// A plain file (config, script) served next to the pack.
    pub fn file(mut self, path: &str, body: &[u8], preserve: bool) -> Self {
        self.entry(path, body, false, preserve);
        self
    }

    /// A mod downloaded from a URL (Modrinth style, sha512). `optional` is `Some(default)`.
    pub fn url_mod(
        mut self,
        slug: &str,
        file_name: &str,
        body: &[u8],
        side: &str,
        optional: Option<bool>,
    ) -> Self {
        let mut meta = format!(
            "name = \"{slug}\"\nfilename = \"{file_name}\"\nside = \"{side}\"\n\n[download]\n\
             url = \"{}/files/{file_name}\"\nhash-format = \"sha512\"\nhash = \"{}\"\n",
            self.base,
            sha512_hex(body)
        );
        if let Some(default) = optional {
            meta += &format!(
                "\n[option]\noptional = true\ndefault = {default}\ndescription = \"Optional {slug}\"\n"
            );
        }
        self.hosted.push((format!("/files/{file_name}"), body.to_vec()));
        self.entry(&format!("mods/{slug}.pw.toml"), meta.as_bytes(), true, false);
        self
    }

    /// A CurseForge mod (`metadata:curseforge`). `blocked` leaves it off the CDN, like a mod
    /// whose author blocks other launchers. Use file names that need no URL escaping.
    pub fn cf_mod(
        mut self,
        slug: &str,
        file_name: &str,
        file_id: u64,
        project_id: u64,
        body: &[u8],
        blocked: bool,
    ) -> Self {
        let meta = format!(
            "name = \"{slug}\"\nfilename = \"{file_name}\"\nside = \"both\"\n\n[download]\n\
             hash-format = \"sha1\"\nhash = \"{}\"\nmode = \"metadata:curseforge\"\n\n\
             [update]\n[update.curseforge]\nfile-id = {file_id}\nproject-id = {project_id}\n",
            sha1_hex(body)
        );
        if !blocked {
            self.hosted.push((
                format!("/files/{}/{}/{file_name}", file_id / 1000, file_id % 1000),
                body.to_vec(),
            ));
        }
        self.entry(&format!("mods/{slug}.pw.toml"), meta.as_bytes(), true, false);
        self
    }

    pub fn sidecar(mut self, toml: &str) -> Self {
        self.hosted.push(("/pack/launcher.toml".into(), toml.as_bytes().to_vec()));
        self
    }

    /// `pack.toml` names a wrong hash for `index.toml`.
    pub fn corrupt_index(mut self) -> Self {
        self.corrupt_index = true;
        self
    }

    pub fn pack_toml(&self) -> String {
        let index_hash = if self.corrupt_index {
            sha256_hex(b"something else")
        } else {
            sha256_hex(self.index.as_bytes())
        };
        let mut text = format!(
            "name = \"{}\"\nversion = \"{}\"\npack-format = \"packwiz:1.1.0\"\n\n[index]\n\
             file = \"index.toml\"\nhash-format = \"sha256\"\nhash = \"{index_hash}\"\n\n\
             [versions]\nminecraft = \"1.20.1\"\n",
            self.name, self.version
        );
        if let Some((kind, version)) = &self.loader {
            text += &format!("{kind} = \"{version}\"\n");
        }
        text
    }

    /// Serve every file and return the `pack.toml` URL. To publish a new version, call
    /// `server.reset().await` and mount the new builder.
    pub async fn mount(&self, server: &MockServer) -> String {
        let mut all = self.hosted.clone();
        all.push(("/pack/index.toml".into(), self.index.clone().into_bytes()));
        all.push(("/pack/pack.toml".into(), self.pack_toml().into_bytes()));
        for (at, body) in all {
            Mock::given(method("GET"))
                .and(path(at))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(server)
                .await;
        }
        format!("{}/pack/pack.toml", self.base)
    }
}

pub fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, body) in entries {
        w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(body).unwrap();
    }
    w.finish().unwrap();
}
```

- [ ] **Step 2: Write the failing tests** (`crates/launcher-core/tests/packwiz.rs`)

```rust
mod common;

use common::{sources, PackBuilder};
use launcher_core::{
    download::hash::sha256_hex,
    instances::{SourceRef, UpdatePolicy},
    meta::loaders::LoaderKind,
    sources::{packwiz, FileMode, FileSource, Side, UpdateCheck},
    ErrorCode,
};
use wiremock::MockServer;

fn tiny(server: &MockServer) -> PackBuilder {
    PackBuilder::new(server, "Tiny", "1.0.0")
        .url_mod("appleskin", "appleskin.jar", b"apple", "both", None)
        .url_mod("zoom", "zoom.jar", b"zoom", "client", None)
        .url_mod("backups", "backups.jar", b"backup", "server", None)
        .url_mod("shaders", "shaders.jar", b"shade", "client", Some(false))
        .cf_mod("cozy", "cozy.jar", 1234567, 42, b"cozy", false)
        .file("config/a.toml", b"a = 1", true)
}

#[tokio::test]
async fn resolves_a_tiny_pack() {
    let server = MockServer::start().await;
    let pack = tiny(&server);
    let url = pack.mount(&server).await;
    let tmp = tempfile::tempdir().unwrap();
    let desired = packwiz::resolve(&sources(&server, tmp.path()), &url).await.unwrap();

    assert_eq!(desired.name, "Tiny");
    assert_eq!(desired.pack_version, "1.0.0");
    assert_eq!(desired.minecraft, "1.20.1");
    let loader = desired.loader.clone().unwrap();
    assert_eq!((loader.kind, loader.version.as_str()), (LoaderKind::Forge, "47.4.22"));
    assert_eq!(desired.stamp, sha256_hex(pack.pack_toml().as_bytes()));

    let by_path = |p: &str| desired.files.iter().find(|f| f.path == p).unwrap().clone();
    let apple = by_path("mods/appleskin.jar");
    assert_eq!(
        apple.source,
        FileSource::Remote { urls: vec![format!("{}/files/appleskin.jar", server.uri())] }
    );
    assert!(apple.hashes.sha512.is_some());
    assert_eq!(by_path("mods/zoom.jar").side, Side::Client);
    assert_eq!(by_path("mods/backups.jar").side, Side::Server);
    assert!(!by_path("mods/shaders.jar").optional.unwrap().default);
    let cozy = by_path("mods/cozy.jar");
    assert_eq!(
        cozy.source,
        FileSource::Remote { urls: vec![format!("{}/files/1234/567/cozy.jar", server.uri())] }
    );
    assert_eq!(cozy.manual.unwrap().page_url, "https://www.curseforge.com/projects/42");
    let config = by_path("config/a.toml");
    assert_eq!(config.mode, FileMode::Preserve);
    assert_eq!(
        config.source,
        FileSource::Remote { urls: vec![format!("{}/pack/config/a.toml", server.uri())] }
    );
    assert_eq!(desired.files.len(), 6);
}

#[tokio::test]
async fn reads_the_sidecar_when_there_is_one() {
    let server = MockServer::start().await;
    let url = tiny(&server)
        .sidecar("[art]\nicon = \"assets/icon.png\"\n[sync]\npolicy = \"auto\"\n")
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let desired = packwiz::resolve(&sources(&server, tmp.path()), &url).await.unwrap();
    assert_eq!(
        desired.about.art.icon,
        Some(format!("{}/pack/assets/icon.png", server.uri()))
    );
    assert_eq!(desired.about.policy, Some(UpdatePolicy::Auto));

    let plain = MockServer::start().await;
    let url = tiny(&plain).mount(&plain).await;
    let tmp = tempfile::tempdir().unwrap();
    let desired = packwiz::resolve(&sources(&plain, tmp.path()), &url).await.unwrap();
    assert_eq!(desired.about, Default::default(), "404 means no sidecar");
}

#[tokio::test]
async fn a_wrong_index_hash_is_refused() {
    let server = MockServer::start().await;
    let url = tiny(&server).corrupt_index().mount(&server).await;
    let tmp = tempfile::tempdir().unwrap();
    let err = packwiz::resolve(&sources(&server, tmp.path()), &url).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaChecksum);
}

#[tokio::test]
async fn packwiz_rejects_escaping_and_colliding_paths() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let url = PackBuilder::new(&server, "Evil", "1")
        .file("../escape.txt", b"x", false)
        .mount(&server)
        .await;
    let err = packwiz::resolve(&sources(&server, tmp.path()), &url).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PackUnsafePath);

    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let url = PackBuilder::new(&server, "Clash", "1")
        .file("config/A.toml", b"x", false)
        .file("config/a.toml", b"y", false)
        .mount(&server)
        .await;
    let err = packwiz::resolve(&sources(&server, tmp.path()), &url).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PackUnsafePath);
}

#[tokio::test]
async fn resolves_offline_from_the_cache() {
    let server = MockServer::start().await;
    let url = tiny(&server).mount(&server).await;
    let tmp = tempfile::tempdir().unwrap();
    let src = sources(&server, tmp.path());
    let online = packwiz::resolve(&src, &url).await.unwrap();
    drop(server); // connections are now refused
    let offline = packwiz::resolve(&src, &url).await.unwrap();
    assert_eq!(online, offline);
}

#[tokio::test]
async fn a_missing_pack_is_not_found() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let err = packwiz::resolve(
        &sources(&server, tmp.path()),
        &format!("{}/nothing/pack.toml", server.uri()),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::PackNotFound);
}

#[tokio::test]
async fn check_compares_the_stamp_and_fails_softly() {
    let server = MockServer::start().await;
    let v1 = tiny(&server);
    let url = v1.mount(&server).await;
    let tmp = tempfile::tempdir().unwrap();
    let src = sources(&server, tmp.path());
    let stamp = sha256_hex(v1.pack_toml().as_bytes());
    assert_eq!(packwiz::check(&src, &url, &stamp).await, UpdateCheck::UpToDate);

    server.reset().await;
    let v2 = tiny(&server).with_version("1.0.1");
    v2.mount(&server).await;
    assert_eq!(
        packwiz::check(&src, &url, &stamp).await,
        UpdateCheck::Available {
            source: SourceRef::Packwiz { url: url.clone() },
            stamp: sha256_hex(v2.pack_toml().as_bytes()),
            version: "1.0.1".into(),
        }
    );

    drop(server);
    match packwiz::check(&src, &url, &stamp).await {
        UpdateCheck::Unavailable(e) => assert_eq!(e.code, ErrorCode::UpdateCheckFailed),
        other => panic!("{other:?}"),
    }
}
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -p launcher-core --test packwiz`
Expected: compile errors (`Sources`, `packwiz::resolve` and `packwiz::check` not found).

- [ ] **Step 4: Implement**

In `sources/mod.rs`, add the facade struct (the facade methods come in Task 11):

```rust
use crate::{download::{cache::ObjectCache, Downloader}, meta::MetaClient};

/// Everything a source needs to fetch a pack.
#[derive(Clone)]
pub struct Sources {
    pub http: reqwest::Client,
    pub meta: MetaClient,
    pub downloader: Downloader,
    pub cache: ObjectCache,
    pub config: SourceConfig,
}
```

Append to `sources/packwiz.rs` (add the imports `futures_util::{StreamExt, TryStreamExt}`, `crate::download::{hash::sha256_hex, host_of}`, `crate::error::{ErrorCode, LauncherError}`, `crate::instances::SourceRef`, and `super::{check_pack_url, game_from, sidecar::parse_sidecar, DesiredState, PackAbout, Sources, UpdateCheck}`):

```rust
/// How many metafiles are fetched at once.
const METAFILE_CONCURRENCY: usize = 8;

fn pack_not_found(e: LauncherError) -> LauncherError {
    if e.code == ErrorCode::MetaNotFound {
        LauncherError::pack(ErrorCode::PackNotFound, "That pack wasn't found")
            .with_fix("Check the link and try again")
            .with_detail(e.message)
    } else {
        e
    }
}

/// The pack as it is now. Every byte goes through `MetaClient`: `pack.toml` is revalidated with
/// its ETag, and `index.toml` and the metafiles are checked against their hashes, so unchanged
/// metafiles are never downloaded again and an offline resolve works from the cache.
pub async fn resolve(src: &Sources, pack_url: &str) -> Result<DesiredState> {
    let url = check_pack_url(pack_url)?;
    let pack_bytes = src
        .meta
        .get_bytes_validated(url.as_str(), |b| parse_pack(b).map(|_| ()))
        .await
        .map_err(pack_not_found)?;
    let pack = parse_pack(&pack_bytes)?;

    let index_file = normalize_rel(&pack.index.file).ok_or_else(|| unsafe_path(&pack.index.file))?;
    let index_url = url
        .join(&encode_path(&index_file))
        .map_err(|e| invalid("pack.toml", e))?;
    let index_hashes = entry_hashes(&pack.index.hash_format, &pack.index.hash, "pack.toml")?;
    let index_bytes = src.meta.get_verified_bytes(index_url.as_str(), &index_hashes).await?;
    let index = parse_index(&index_bytes)?;

    let mut files: Vec<PackFile> = futures_util::stream::iter(
        index
            .files
            .iter()
            .map(|entry| index_entry(src, &index_url, entry, &index.hash_format)),
    )
    .buffered(METAFILE_CONCURRENCY)
    .try_collect()
    .await?;
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let (minecraft, loader) = game_from(&pack.versions)?;
    let name = match pack.name.trim() {
        "" => "Pack".to_string(),
        n => n.chars().take(64).collect(),
    };
    let desired = DesiredState {
        name,
        pack_version: pack.version.unwrap_or_default(),
        minecraft,
        loader,
        files,
        stamp: sha256_hex(&pack_bytes),
        about: fetch_sidecar(src, &url).await,
        archive: None,
    };
    desired.check_paths()?;
    Ok(desired)
}

async fn index_entry(
    src: &Sources,
    index_url: &reqwest::Url,
    entry: &IndexEntry,
    default_format: &str,
) -> Result<PackFile> {
    if !entry.metafile {
        return plain_file(index_url, entry, default_format);
    }
    let file = normalize_rel(&entry.file).ok_or_else(|| unsafe_path(&entry.file))?;
    let format = entry.hash_format.as_deref().unwrap_or(default_format);
    let hashes = entry_hashes(format, &entry.hash, &file)?;
    let url = index_url
        .join(&encode_path(&file))
        .map_err(|e| invalid(&file, e))?;
    let bytes = src.meta.get_verified_bytes(url.as_str(), &hashes).await?;
    let meta = parse_meta(&bytes, &file)?;
    meta_file(&src.config, &file, &meta, entry.preserve)
}

/// `launcher.toml` next to `pack.toml`; anything other than a readable file means "none".
async fn fetch_sidecar(src: &Sources, pack_url: &reqwest::Url) -> PackAbout {
    let Ok(url) = pack_url.join("launcher.toml") else {
        return PackAbout::default();
    };
    match src.meta.get_bytes(url.as_str()).await {
        Ok(bytes) => std::str::from_utf8(&bytes)
            .map(|text| parse_sidecar(text, pack_url.as_str()))
            .unwrap_or_default(),
        Err(_) => PackAbout::default(),
    }
}

/// The cheap check (spec §3.1): fetch `pack.toml` fresh and compare its sha256. Never fails:
/// problems come back as `Unavailable` so the player can still play what they have.
pub async fn check(src: &Sources, pack_url: &str, installed_stamp: &str) -> UpdateCheck {
    let unavailable = |detail: String| {
        UpdateCheck::Unavailable(
            LauncherError::network(ErrorCode::UpdateCheckFailed, "Couldn't check for updates")
                .with_fix("You can still play the version you have")
                .with_detail(detail),
        )
    };
    let url = match check_pack_url(pack_url) {
        Ok(url) => url,
        Err(e) => return UpdateCheck::Unavailable(e),
    };
    let response = match src.http.get(url.as_str()).send().await {
        Ok(r) => r,
        Err(e) => return unavailable(e.without_url().to_string()),
    };
    if !response.status().is_success() {
        return unavailable(format!("{} answered {}", host_of(url.as_str()), response.status()));
    }
    let bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => return unavailable(e.without_url().to_string()),
    };
    let pack = match parse_pack(&bytes) {
        Ok(p) => p,
        Err(e) => return UpdateCheck::Unavailable(e),
    };
    let stamp = sha256_hex(&bytes);
    if stamp == installed_stamp {
        UpdateCheck::UpToDate
    } else {
        UpdateCheck::Available {
            source: SourceRef::Packwiz { url: pack_url.to_string() },
            stamp,
            version: pack.version.unwrap_or_default(),
        }
    }
}
```

If `MetaClient::get_bytes_validated` is still `pub(crate)`, it stays that way: `sources` is in the same crate.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p launcher-core --test packwiz`
Expected: PASS (7 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/launcher-core/src/sources crates/launcher-core/tests/common crates/launcher-core/tests/packwiz.rs
git commit -m "feat(core): resolve packwiz packs through the verified metadata cache; cheap update check

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Reading `.mrpack` files

**Files:**
- Create: `crates/launcher-core/src/sources/mrpack.rs`
- Modify: `crates/launcher-core/src/sources/mod.rs` (`pub mod mrpack;`)
- Test: `crates/launcher-core/tests/mrpack.rs`

**Interfaces:**
- Consumes: `normalize_rel`, `Hashes::from_format`, `sha1_file`, `ObjectCache::insert_file`/`get`, `game_from`, `check_pack_url`, the error helpers.
- Produces, in `launcher_core::sources::mrpack`:
  - `MrIndex { format_version: u32, game, version_id, name, summary: Option<String>, files: Vec<MrFile>, dependencies: BTreeMap<String, String> }`
  - `MrFile { path, hashes: MrHashes { sha1, sha512: Option<String> }, env: Option<MrEnv { client, server: Option<String> }>, downloads: Vec<String>, file_size: Option<u64> }`
  - `parse_index(&[u8]) -> Result<MrIndex>`: `formatVersion` must be 1 and `game` must be `minecraft`; anything else is `PackUnsupported`.
  - `Limits { index: u64, entry: u64, total: u64 }`. `Default` is 16 MiB, 1 GiB and 8 GiB.
  - `read_archive(path: &Path, stamp: String, archive: Hashes, limits: Limits) -> Result<DesiredState>` (blocking):
    - Index files: `env.client = unsupported` → `Side::Server`; `env.server = unsupported` → `Side::Client`; `env.client = optional` → `optional: Some({ default: true })`.
    - Overrides: `overrides/` then `client-overrides/` (the later one wins; both win over an index file at the same path). Each becomes `FileSource::Archive { entry }` with sha1 and size. The mode is `Replace` under `mods/` and `ReplaceUnlessModified` elsewhere.
  - `extract_entry(archive: &Path, entry: &str, dest: &Path, sha1: &str) -> Result<()>` (blocking): copies one entry and checks its sha1.
  - `import_file(src: &Sources, path: &Path) -> Result<(SourceRef, DesiredState)>` (async): caches the file and reads it as `SourceRef::Mrpack { name: <file name>, sha1 }`, with the stamp equal to the sha1.
  - `resolve_cached(src: &Sources, sha1: &str) -> Result<DesiredState>` (async): `PackNotFound` if the cached archive is gone.

- [ ] **Step 1: Write the failing tests** (`crates/launcher-core/tests/mrpack.rs`)

```rust
mod common;

use common::{sources, write_zip};
use launcher_core::{
    download::{hash::sha1_hex, Hashes},
    instances::SourceRef,
    meta::loaders::LoaderKind,
    sources::{
        mrpack::{self, Limits},
        FileMode, FileSource, Side,
    },
    ErrorCode,
};
use serde_json::json;
use wiremock::MockServer;

fn index(files: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": "2.1.0",
        "name": "Sky Pack",
        "summary": "Islands",
        "files": files,
        "dependencies": { "minecraft": "1.20.1", "fabric-loader": "0.16.14" }
    }))
    .unwrap()
}

fn file(path: &str, env: Option<serde_json::Value>) -> serde_json::Value {
    let mut f = json!({
        "path": path,
        "hashes": { "sha1": sha1_hex(path.as_bytes()), "sha512": null },
        "downloads": [format!("https://cdn.example.com/{path}")],
        "fileSize": 10
    });
    if let Some(env) = env {
        f["env"] = env;
    }
    f
}

#[tokio::test]
async fn imports_files_env_and_overrides() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let pack = tmp.path().join("Sky Pack-2.1.0.mrpack");
    let idx = index(json!([
        file("mods/a.jar", None),
        file("mods/server.jar", Some(json!({ "client": "unsupported", "server": "required" }))),
        file("mods/opt.jar", Some(json!({ "client": "optional", "server": "unsupported" }))),
        file("config/x.toml", None),
    ]));
    write_zip(
        &pack,
        &[
            ("modrinth.index.json", &idx[..]),
            ("overrides/config/x.toml", &b"shared"[..]),
            ("overrides/mods/extra.jar", &b"jar"[..]),
            ("client-overrides/config/x.toml", &b"client"[..]),
            ("server-overrides/config/s.toml", &b"ignored"[..]),
        ],
    );
    let src = sources(&server, tmp.path());
    let (source, desired) = mrpack::import_file(&src, &pack).await.unwrap();
    let sha1 = sha1_hex(&std::fs::read(&pack).unwrap());
    assert_eq!(
        source,
        SourceRef::Mrpack { name: "Sky Pack-2.1.0.mrpack".into(), sha1: sha1.clone() }
    );
    assert_eq!(desired.stamp, sha1);
    assert_eq!(desired.archive, Some(Hashes::sha1(&sha1)));
    assert_eq!((desired.name.as_str(), desired.pack_version.as_str()), ("Sky Pack", "2.1.0"));
    assert_eq!(desired.about.description.as_deref(), Some("Islands"));
    assert_eq!(desired.loader.clone().unwrap().kind, LoaderKind::Fabric);

    let by_path = |p: &str| desired.files.iter().find(|f| f.path == p).cloned();
    assert_eq!(by_path("mods/server.jar").unwrap().side, Side::Server);
    let opt = by_path("mods/opt.jar").unwrap();
    assert_eq!(opt.side, Side::Client);
    assert!(opt.optional.unwrap().default);
    let x = by_path("config/x.toml").unwrap();
    assert_eq!(x.source, FileSource::Archive { entry: "client-overrides/config/x.toml".into() });
    assert_eq!(x.hashes, Hashes::sha1(sha1_hex(b"client")));
    assert_eq!(x.mode, FileMode::ReplaceUnlessModified);
    let extra = by_path("mods/extra.jar").unwrap();
    assert_eq!(extra.mode, FileMode::Replace);
    assert_eq!(extra.size, Some(3));
    assert!(by_path("config/s.toml").is_none(), "server overrides never reach a client");

    let again = mrpack::resolve_cached(&src, &sha1).await.unwrap();
    assert_eq!(again, desired);

    let out = tmp.path().join("out/x.toml");
    let archive = src.cache.get(&Hashes::sha1(&sha1)).await.unwrap();
    mrpack::extract_entry(&archive, "client-overrides/config/x.toml", &out, &sha1_hex(b"client")).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"client");
    let wrong = mrpack::extract_entry(&archive, "overrides/config/x.toml", &out, &sha1_hex(b"client"));
    assert_eq!(wrong.unwrap_err().code, ErrorCode::PackInvalid);
}

#[test]
fn mrpack_rejects_unsafe_paths_and_oversized_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let read = |entries: &[(&str, &[u8])], limits: Limits| {
        let p = tmp.path().join("p.mrpack");
        write_zip(&p, entries);
        mrpack::read_archive(&p, "s".into(), Hashes::sha1(sha1_hex(b"s")), limits)
    };
    let escaping = index(json!([file("../evil.jar", None)]));
    assert_eq!(
        read(&[("modrinth.index.json", &escaping[..])], Limits::default()).unwrap_err().code,
        ErrorCode::PackUnsafePath
    );
    let ok = index(json!([]));
    assert_eq!(
        read(&[("modrinth.index.json", &ok[..]), ("overrides/CON.txt", &b"x"[..])], Limits::default())
            .unwrap_err()
            .code,
        ErrorCode::PackUnsafePath
    );
    let small = Limits { index: 1 << 20, entry: 4, total: 100 };
    assert_eq!(
        read(&[("modrinth.index.json", &ok[..]), ("overrides/big.txt", &b"0123456789"[..])], small)
            .unwrap_err()
            .code,
        ErrorCode::PackInvalid
    );
    let tiny_total = Limits { index: 1 << 20, entry: 8, total: 10 };
    assert_eq!(
        read(
            &[("modrinth.index.json", &ok[..]), ("overrides/a.txt", &b"123456"[..]), ("overrides/b.txt", &b"123456"[..])],
            tiny_total
        )
        .unwrap_err()
        .code,
        ErrorCode::PackInvalid
    );
    let tiny_index = Limits { index: 8, entry: 8, total: 10 };
    assert_eq!(read(&[("modrinth.index.json", &ok[..])], tiny_index).unwrap_err().code, ErrorCode::PackInvalid);
}

#[test]
fn mrpack_format_problems_are_named() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("p.mrpack");
    write_zip(&p, &[("readme.txt", &b"no index"[..])]);
    let read = || mrpack::read_archive(&p, "s".into(), Hashes::default(), Limits::default());
    assert_eq!(read().unwrap_err().code, ErrorCode::PackInvalid);

    let mut other_game: serde_json::Value = serde_json::from_slice(&index(json!([]))).unwrap();
    other_game["game"] = json!("terraria");
    write_zip(&p, &[("modrinth.index.json", &serde_json::to_vec(&other_game).unwrap()[..])]);
    assert_eq!(read().unwrap_err().code, ErrorCode::PackUnsupported);

    let mut plain_http = file("mods/a.jar", None);
    plain_http["downloads"] = json!(["http://cdn.example.com/a.jar"]);
    write_zip(&p, &[("modrinth.index.json", &index(json!([plain_http]))[..])]);
    assert_eq!(read().unwrap_err().code, ErrorCode::PackInvalid, "no usable https download");

    std::fs::write(&p, b"not a zip").unwrap();
    assert_eq!(read().unwrap_err().code, ErrorCode::PackInvalid);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --test mrpack`
Expected: compile error, no module `mrpack`.

- [ ] **Step 3: Implement** `crates/launcher-core/src/sources/mrpack.rs` (and `pub mod mrpack;`)

```rust
//! Modrinth's `.mrpack`: a zip holding `modrinth.index.json` (files to download) plus
//! `overrides/` and `client-overrides/` copied into the game folder. Written from the published
//! format. Archive contents are untrusted: paths go through `normalize_rel`, and sizes are
//! capped and checked while reading, so a zip that lies about its sizes can't run away.
use std::{collections::BTreeMap, io::Read, path::Path};

use serde::Deserialize;
use sha1::{Digest, Sha1};

use super::{
    check_pack_url, game_from, invalid, unsafe_path, unsupported, DesiredState, FileMode,
    FileSource, OptionalFile, PackAbout, PackFile, Side, Sources,
};
use crate::{
    download::{hash::sha1_file, Hashes},
    error::{ErrorCode, LauncherError, Result},
    fsutil::normalize_rel,
    instances::SourceRef,
};

const INDEX: &str = "modrinth.index.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrIndex {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub files: Vec<MrFile>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrFile {
    pub path: String,
    pub hashes: MrHashes,
    #[serde(default)]
    pub env: Option<MrEnv>,
    #[serde(default)]
    pub downloads: Vec<String>,
    #[serde(default)]
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MrHashes {
    pub sha1: String,
    #[serde(default)]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MrEnv {
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
}

/// Size caps for reading an archive, in bytes.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub index: u64,
    pub entry: u64,
    pub total: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            index: 16 << 20,
            entry: 1 << 30,
            total: 8 << 30,
        }
    }
}

pub fn parse_index(bytes: &[u8]) -> Result<MrIndex> {
    let index: MrIndex = serde_json::from_slice(bytes).map_err(|e| invalid(INDEX, e))?;
    if index.format_version != 1 {
        return Err(unsupported(&format!("mrpack format {}", index.format_version)));
    }
    if index.game != "minecraft" {
        return Err(unsupported(&format!("packs for {}", index.game)));
    }
    Ok(index)
}

fn env_of(env: Option<&MrEnv>) -> (Side, Option<OptionalFile>) {
    let Some(env) = env else {
        return (Side::Both, None);
    };
    let side = match (env.client.as_deref(), env.server.as_deref()) {
        (Some("unsupported"), _) => Side::Server,
        (_, Some("unsupported")) => Side::Client,
        _ => Side::Both,
    };
    let optional = (env.client.as_deref() == Some("optional")).then_some(OptionalFile {
        default: true,
        description: None,
    });
    (side, optional)
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn index_files(index: &MrIndex) -> Result<Vec<PackFile>> {
    index
        .files
        .iter()
        .map(|f| {
            let path = normalize_rel(&f.path).ok_or_else(|| unsafe_path(&f.path))?;
            let mut hashes = Hashes::from_format("sha1", &f.hashes.sha1)
                .ok_or_else(|| invalid(INDEX, format!("{path}: malformed sha1")))?;
            if let Some(sha512) = &f.hashes.sha512 {
                hashes.sha512 = Hashes::from_format("sha512", sha512)
                    .and_then(|h| h.sha512)
                    .ok_or_else(|| invalid(INDEX, format!("{path}: malformed sha512")))?
                    .into();
            }
            let urls: Vec<String> = f
                .downloads
                .iter()
                .filter(|u| check_pack_url(u).is_ok())
                .cloned()
                .collect();
            if urls.is_empty() {
                return Err(invalid(INDEX, format!("{path} has no https download")));
            }
            let (side, optional) = env_of(f.env.as_ref());
            Ok(PackFile {
                name: file_name(&path),
                path,
                source: FileSource::Remote { urls },
                hashes,
                size: f.file_size,
                side,
                optional,
                mode: FileMode::Replace,
                manual: None,
            })
        })
        .collect()
}

fn zip_error(e: zip::result::ZipError) -> LauncherError {
    invalid("the .mrpack", e)
}

/// The sha1 and length of everything `reader` yields.
fn hash_reader(mut reader: impl Read) -> std::io::Result<(String, u64)> {
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((hex::encode(hasher.finalize()), total))
}

/// Read a `.mrpack`: the files its index lists plus its override entries.
pub fn read_archive(
    path: &Path,
    stamp: String,
    archive: Hashes,
    limits: Limits,
) -> Result<DesiredState> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(zip_error)?;
    let index = {
        let entry = zip.by_name(INDEX).map_err(|e| invalid(INDEX, e))?;
        let mut bytes = Vec::new();
        entry.take(limits.index + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limits.index {
            return Err(invalid(INDEX, "too large"));
        }
        parse_index(&bytes)?
    };
    let mut files: BTreeMap<String, PackFile> = index_files(&index)?
        .into_iter()
        .map(|f| (f.path.clone(), f))
        .collect();
    let mut total = 0u64;
    for prefix in ["overrides/", "client-overrides/"] {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(zip_error)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            let Some(rel) = name.strip_prefix(prefix) else {
                continue;
            };
            let path = normalize_rel(rel).ok_or_else(|| unsafe_path(rel))?;
            let declared = entry.size();
            total = total.saturating_add(declared);
            if declared > limits.entry || total > limits.total {
                return Err(invalid(&path, "larger than a pack file may be"));
            }
            let (sha1, size) = hash_reader((&mut entry).take(declared + 1))?;
            if size != declared {
                return Err(invalid(&path, "its size doesn't match the archive"));
            }
            let mode = if path.starts_with("mods/") {
                FileMode::Replace
            } else {
                FileMode::ReplaceUnlessModified
            };
            files.insert(
                path.clone(),
                PackFile {
                    name: file_name(&path),
                    path,
                    source: FileSource::Archive { entry: name },
                    hashes: Hashes::sha1(sha1),
                    size: Some(size),
                    side: Side::Both,
                    optional: None,
                    mode,
                    manual: None,
                },
            );
        }
    }
    let (minecraft, loader) = game_from(&index.dependencies)?;
    let desired = DesiredState {
        name: index.name.trim().chars().take(64).collect(),
        pack_version: index.version_id,
        minecraft,
        loader,
        files: files.into_values().collect(),
        stamp,
        about: PackAbout {
            description: index.summary,
            ..PackAbout::default()
        },
        archive: Some(archive),
    };
    desired.check_paths()?;
    Ok(desired)
}

/// Copy one archive entry to `dest`, checking its sha1 on the way (the cached archive could
/// have been damaged since it was read).
pub fn extract_entry(archive: &Path, entry: &str, dest: &Path, sha1: &str) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?).map_err(zip_error)?;
    let mut source = zip.by_name(entry).map_err(|e| invalid(entry, e))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::fs::File::create(dest)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = source.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        std::io::Write::write_all(&mut out, &buf[..n])?;
    }
    drop(out);
    if !hex::encode(hasher.finalize()).eq_ignore_ascii_case(sha1) {
        let _ = std::fs::remove_file(dest);
        return Err(invalid(entry, "it changed since the pack was read"));
    }
    Ok(())
}

fn join_error(e: tokio::task::JoinError) -> LauncherError {
    std::io::Error::other(e).into()
}

/// A `.mrpack` the player picked: keep a copy in the object cache and read it.
pub async fn import_file(src: &Sources, path: &Path) -> Result<(SourceRef, DesiredState)> {
    let p = path.to_path_buf();
    let sha1 = tokio::task::spawn_blocking(move || sha1_file(&p))
        .await
        .map_err(join_error)??;
    src.cache.insert_file(path, &Hashes::sha1(&sha1)).await?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pack.mrpack".into());
    let desired = resolve_cached(src, &sha1).await?;
    Ok((SourceRef::Mrpack { name, sha1 }, desired))
}

/// Read an imported `.mrpack` again from the object cache.
pub async fn resolve_cached(src: &Sources, sha1: &str) -> Result<DesiredState> {
    let hashes = Hashes::sha1(sha1);
    let path = src.cache.get(&hashes).await.ok_or_else(|| {
        LauncherError::pack(ErrorCode::PackNotFound, "The imported pack file is gone")
            .with_fix("Import the .mrpack file again")
    })?;
    let stamp = sha1.to_string();
    tokio::task::spawn_blocking(move || read_archive(&path, stamp, hashes, Limits::default()))
        .await
        .map_err(join_error)?
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --test mrpack`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/sources crates/launcher-core/tests/mrpack.rs
git commit -m "feat(core): read .mrpack files with capped, path-checked overrides

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Modrinth modpacks, and the `Sources` facade

**Files:**
- Create: `crates/launcher-core/src/sources/modrinth.rs`
- Modify: `crates/launcher-core/src/sources/mod.rs` (`pub mod modrinth;`, facade methods, `no_pack()`)
- Test: `crates/launcher-core/tests/modrinth.rs`; unit tests for `pick_update` in `modrinth.rs`

**Interfaces:**
- Consumes: `mrpack::read_archive`, `ObjectCache::ensure`, `MetaClient::get_json`, `valid_modrinth_id`, `check_pack_url`.
- Produces, in `launcher_core::sources::modrinth`:
  - The API types: `Project { id, slug, title, description: Option<String>, project_type, icon_url: Option<String>, gallery: Vec<GalleryImage { url, featured }> }`, `Version { id, project_id, name, version_number, version_type, game_versions: Vec<String>, date_published, files: Vec<VersionFile { url, filename, primary, size, hashes: VersionHashes { sha1, sha512: Option<String> } }> }`.
  - `project(src, id) -> Result<Project>` and `version(src, id) -> Result<Version>`. Both go through the cached `MetaClient`, and a 404 is `PackNotFound`.
  - `versions(src, project_id, minecraft: Option<&str>) -> Result<Vec<Version>>`: always fresh.
  - `pick_update<'a>(&'a [Version], installed_type: &str) -> Option<&'a Version>`: the newest by `date_published` among versions at least as stable as `installed_type` (release < beta < alpha).
  - `resolve(src, project_id, version_id) -> Result<DesiredState>`:
    - The project must be a modpack; otherwise `PackUnsupported`.
    - It downloads the version's `.mrpack` into the object cache and reads it.
    - The name is the project title and the version is `version_number`.
    - The art is the project icon plus the featured gallery image (else the first).
    - The stamp is the version id.
  - `open(src, project, version: Option<&str>) -> Result<(SourceRef, DesiredState)>`. With no version, it picks the newest release, else the newest of any type.
  - `check(src, project_id, installed_version_id, minecraft) -> UpdateCheck`.
- Produces, on `Sources`:
  - `open(&self, &PackInput) -> Result<(SourceRef, DesiredState)>`
  - `resolve(&self, &SourceRef) -> Result<DesiredState>`: `NoPack` for `SourceRef::None`.
  - `check(&self, &SourceRef, installed: &DesiredState) -> UpdateCheck`: `.mrpack` is always `UpToDate`.

- [ ] **Step 1: Write the failing tests**

The test module in `crates/launcher-core/src/sources/modrinth.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: &str, kind: &str, date: &str) -> Version {
        Version {
            id: id.into(),
            project_id: "p".into(),
            name: id.into(),
            version_number: id.into(),
            version_type: kind.into(),
            game_versions: vec!["1.20.1".into()],
            date_published: date.into(),
            files: vec![],
        }
    }

    #[test]
    fn updates_never_get_less_stable() {
        let list = vec![
            v("alpha-new", "alpha", "2024-03-01T00:00:00Z"),
            v("beta-mid", "beta", "2024-02-01T00:00:00Z"),
            v("release-old", "release", "2024-01-01T00:00:00Z"),
        ];
        assert_eq!(pick_update(&list, "release").unwrap().id, "release-old");
        assert_eq!(pick_update(&list, "beta").unwrap().id, "beta-mid");
        assert_eq!(pick_update(&list, "alpha").unwrap().id, "alpha-new");
        assert!(pick_update(&list[..1], "release").is_none());
    }
}
```

`crates/launcher-core/tests/modrinth.rs`:

```rust
mod common;

use common::{sources, write_zip};
use launcher_core::{
    download::hash::sha1_hex,
    instances::SourceRef,
    sources::{modrinth, PackInput, UpdateCheck},
    ErrorCode,
};
use serde_json::json;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

async fn mount_json(server: &MockServer, at: &str, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

fn version(server: &MockServer, id: &str, kind: &str, date: &str, sha1: &str, size: u64) -> serde_json::Value {
    json!({
        "id": id, "project_id": "P1", "name": id, "version_number": format!("{id}-num"),
        "version_type": kind, "game_versions": ["1.20.1"], "date_published": date,
        "files": [{
            "url": format!("{}/files/{id}.mrpack", server.uri()), "filename": format!("{id}.mrpack"),
            "primary": true, "size": size, "hashes": { "sha1": sha1, "sha512": null }
        }]
    })
}

/// A project "sky" (id P1) with a release R1 (served) and a newer alpha A1.
async fn sky(server: &MockServer, tmp: &std::path::Path) {
    let pack = tmp.join("r1.mrpack");
    let index = serde_json::to_vec(&json!({
        "formatVersion": 1, "game": "minecraft", "versionId": "1.0", "name": "Sky (index name)",
        "files": [], "dependencies": { "minecraft": "1.20.1", "fabric-loader": "0.16.14" }
    }))
    .unwrap();
    write_zip(&pack, &[("modrinth.index.json", &index[..]), ("overrides/options.txt", &b"fov:1"[..])]);
    let bytes = std::fs::read(&pack).unwrap();
    let sha1 = sha1_hex(&bytes);
    Mock::given(method("GET"))
        .and(path("/files/R1.mrpack"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.clone()))
        .mount(server)
        .await;
    let project = json!({
        "id": "P1", "slug": "sky", "title": "Sky Islands", "description": "Float",
        "project_type": "modpack", "icon_url": "https://cdn.example.com/icon.png",
        "gallery": [
            { "url": "https://cdn.example.com/a.png", "featured": false },
            { "url": "https://cdn.example.com/b.png", "featured": true }
        ]
    });
    mount_json(server, "/v2/project/sky", project.clone()).await;
    mount_json(server, "/v2/project/P1", project).await;
    let r1 = version(server, "R1", "release", "2024-01-01T00:00:00Z", &sha1, bytes.len() as u64);
    let a1 = version(server, "A1", "alpha", "2024-02-01T00:00:00Z", &sha1, bytes.len() as u64);
    mount_json(server, "/v2/version/R1", r1.clone()).await;
    mount_json(server, "/v2/project/P1/version", json!([a1, r1])).await;
}

#[tokio::test]
async fn opens_the_newest_release_of_a_modpack() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    sky(&server, tmp.path()).await;
    let src = sources(&server, tmp.path());
    let (source, desired) = src
        .open(&PackInput::Modrinth { project: "sky".into(), version: None })
        .await
        .unwrap();
    assert_eq!(source, SourceRef::Modrinth { project_id: "P1".into(), version_id: "R1".into() });
    assert_eq!(desired.name, "Sky Islands");
    assert_eq!(desired.pack_version, "R1-num");
    assert_eq!(desired.stamp, "R1");
    assert_eq!(desired.about.description.as_deref(), Some("Float"));
    assert_eq!(desired.about.art.icon.as_deref(), Some("https://cdn.example.com/icon.png"));
    assert_eq!(desired.about.art.banner.as_deref(), Some("https://cdn.example.com/b.png"));
    assert_eq!(desired.files[0].path, "options.txt");
    assert_eq!(src.resolve(&source).await.unwrap(), desired);
}

#[tokio::test]
async fn only_modpacks_open() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    mount_json(&server, "/v2/project/sodium", json!({
        "id": "AANobbMI", "slug": "sodium", "title": "Sodium", "project_type": "mod", "gallery": []
    }))
    .await;
    let src = sources(&server, tmp.path());
    let err = modrinth::open(&src, "sodium", Some("X")).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PackUnsupported);
    let missing = modrinth::open(&src, "nothing-here", None).await.unwrap_err();
    assert_eq!(missing.code, ErrorCode::PackNotFound);
}

#[tokio::test]
async fn check_offers_only_as_stable_updates_on_the_same_minecraft() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    sky(&server, tmp.path()).await;
    let src = sources(&server, tmp.path());
    // Installed: the release R1. The only newer version is an alpha.
    assert_eq!(modrinth::check(&src, "P1", "R1", "1.20.1").await, UpdateCheck::UpToDate);

    let newer = version(&server, "R2", "release", "2024-03-01T00:00:00Z", &sha1_hex(b"x"), 1);
    Mock::given(method("GET"))
        .and(path("/v2/project/P1/version"))
        .and(query_param("game_versions", "[\"1.20.1\"]"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([newer])))
        .with_priority(1)
        .mount(&server)
        .await;
    assert_eq!(
        modrinth::check(&src, "P1", "R1", "1.20.1").await,
        UpdateCheck::Available {
            source: SourceRef::Modrinth { project_id: "P1".into(), version_id: "R2".into() },
            stamp: "R2".into(),
            version: "R2-num".into(),
        }
    );

    drop(server);
    match modrinth::check(&src, "P1", "R1", "1.20.1").await {
        UpdateCheck::Unavailable(e) => assert_eq!(e.code, ErrorCode::UpdateCheckFailed),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn sources_facade_routes_by_kind() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let src = sources(&server, tmp.path());
    assert_eq!(src.resolve(&SourceRef::None).await.unwrap_err().code, ErrorCode::NoPack);
    let pack = tmp.path().join("local.mrpack");
    let index = serde_json::to_vec(&json!({
        "formatVersion": 1, "game": "minecraft", "versionId": "1", "name": "Local",
        "files": [], "dependencies": { "minecraft": "1.20.1" }
    }))
    .unwrap();
    write_zip(&pack, &[("modrinth.index.json", &index[..])]);
    let (source, desired) = src.open(&PackInput::MrpackFile { path: pack }).await.unwrap();
    assert!(matches!(source, SourceRef::Mrpack { .. }));
    assert_eq!(src.check(&source, &desired).await, UpdateCheck::UpToDate);
    assert_eq!(src.resolve(&source).await.unwrap(), desired);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --test modrinth`
Expected: compile errors (no module `modrinth`; no method `open` on `Sources`).

- [ ] **Step 3: Implement** `crates/launcher-core/src/sources/modrinth.rs` (and `pub mod modrinth;`)

```rust
//! Modrinth modpacks through the public API v2 (no key; the launcher's descriptive User-Agent).
//! Only modpack projects become instances; Browse (Plan 3c) handles the rest.
use serde::Deserialize;

use super::{
    check_pack_url, invalid, mrpack, unsupported, valid_modrinth_id, DesiredState, PackAbout,
    PackArt, Sources, UpdateCheck,
};
use crate::{
    download::{host_of, Hashes},
    error::{ErrorCode, LauncherError, Result},
    instances::SourceRef,
};

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub project_type: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub gallery: Vec<GalleryImage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GalleryImage {
    pub url: String,
    #[serde(default)]
    pub featured: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    pub date_published: String,
    #[serde(default)]
    pub files: Vec<VersionFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    pub size: u64,
    pub hashes: VersionHashes,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionHashes {
    pub sha1: String,
    #[serde(default)]
    pub sha512: Option<String>,
}

fn api(src: &Sources, path: &str) -> String {
    format!("{}/{path}", src.config.modrinth_api.trim_end_matches('/'))
}

fn checked_id(id: &str) -> Result<&str> {
    if valid_modrinth_id(id) {
        Ok(id)
    } else {
        Err(LauncherError::pack(ErrorCode::PackLinkInvalid, "That doesn't look like a pack link"))
    }
}

fn not_found(e: LauncherError) -> LauncherError {
    if e.code == ErrorCode::MetaNotFound {
        LauncherError::pack(ErrorCode::PackNotFound, "That pack wasn't found")
            .with_fix("Check the link and try again")
    } else {
        e
    }
}

pub async fn project(src: &Sources, id: &str) -> Result<Project> {
    let url = api(src, &format!("project/{}", checked_id(id)?));
    src.meta.get_json(&url).await.map_err(not_found)
}

pub async fn version(src: &Sources, id: &str) -> Result<Version> {
    let url = api(src, &format!("version/{}", checked_id(id)?));
    src.meta.get_json(&url).await.map_err(not_found)
}

/// A project's versions, optionally only those for one Minecraft version. Always fresh.
pub async fn versions(src: &Sources, project_id: &str, minecraft: Option<&str>) -> Result<Vec<Version>> {
    let mut url = reqwest::Url::parse(&api(src, &format!("project/{}/version", checked_id(project_id)?)))
        .map_err(|e| invalid("Modrinth address", e))?;
    if let Some(mc) = minecraft {
        url.query_pairs_mut().append_pair("game_versions", &format!("[\"{mc}\"]"));
    }
    let host = host_of(url.as_str());
    let response = src.http.get(url).send().await.map_err(|e| {
        LauncherError::network(ErrorCode::HostUnreachable, format!("Couldn't reach {host}"))
            .with_param("host", &host)
            .with_fix("Check your internet connection")
            .with_detail(e.without_url())
    })?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(not_found(LauncherError::loader(ErrorCode::MetaNotFound, "not found")));
    }
    if !status.is_success() {
        return Err(LauncherError::network(ErrorCode::HostTrouble, format!("{host} is having trouble ({status})"))
            .with_param("host", &host)
            .with_param("status", status.as_u16()));
    }
    response.json().await.map_err(|e| {
        LauncherError::loader(ErrorCode::MetaUnexpected, format!("Unexpected data from {host}"))
            .with_detail(e.without_url())
    })
}

fn stability(kind: &str) -> u8 {
    match kind {
        "release" => 0,
        "beta" => 1,
        _ => 2,
    }
}

/// The newest version at least as stable as the installed one.
pub fn pick_update<'a>(versions: &'a [Version], installed_type: &str) -> Option<&'a Version> {
    versions
        .iter()
        .filter(|v| stability(&v.version_type) <= stability(installed_type))
        .max_by(|a, b| a.date_published.cmp(&b.date_published))
}

fn mrpack_file(v: &Version) -> Result<&VersionFile> {
    v.files
        .iter()
        .filter(|f| f.filename.to_ascii_lowercase().ends_with(".mrpack"))
        .max_by_key(|f| f.primary)
        .ok_or_else(|| unsupported("a Modrinth version without a .mrpack file"))
}

fn banner(gallery: &[GalleryImage]) -> Option<String> {
    gallery
        .iter()
        .find(|g| g.featured)
        .or_else(|| gallery.first())
        .map(|g| g.url.clone())
}

pub async fn resolve(src: &Sources, project_id: &str, version_id: &str) -> Result<DesiredState> {
    let project = project(src, project_id).await?;
    if project.project_type != "modpack" {
        return Err(unsupported(&format!("Modrinth {} projects", project.project_type)));
    }
    let version = version(src, version_id).await?;
    if version.project_id != project.id {
        return Err(invalid("Modrinth version", "it belongs to another project"));
    }
    let file = mrpack_file(&version)?;
    if check_pack_url(&file.url).is_err() {
        return Err(unsupported("a .mrpack that isn't served over https"));
    }
    let hashes = Hashes {
        sha1: Hashes::from_format("sha1", &file.hashes.sha1).and_then(|h| h.sha1),
        sha512: file
            .hashes
            .sha512
            .as_deref()
            .and_then(|s| Hashes::from_format("sha512", s))
            .and_then(|h| h.sha512),
        ..Hashes::default()
    };
    if hashes.cache_key().is_none() {
        return Err(invalid("Modrinth version", "no usable checksum"));
    }
    let path = src
        .cache
        .ensure(&src.downloader, &[file.url.clone()], &hashes, Some(file.size))
        .await?;
    let (stamp, archive) = (version.id.clone(), hashes);
    let mut desired = tokio::task::spawn_blocking(move || {
        mrpack::read_archive(&path, stamp, archive, mrpack::Limits::default())
    })
    .await
    .map_err(|e| LauncherError::from(std::io::Error::other(e)))??;
    desired.name = project.title.trim().chars().take(64).collect();
    desired.pack_version = version.version_number;
    desired.about = PackAbout {
        description: project.description,
        art: PackArt {
            icon: project.icon_url,
            banner: banner(&project.gallery),
        },
        policy: None,
    };
    Ok(desired)
}

/// A modpack by slug or id, at `version` or its newest release (else newest of any kind).
pub async fn open(src: &Sources, project_ref: &str, version: Option<&str>) -> Result<(SourceRef, DesiredState)> {
    let p = project(src, project_ref).await?;
    if p.project_type != "modpack" {
        return Err(unsupported(&format!("Modrinth {} projects", p.project_type)));
    }
    let version_id = match version {
        Some(v) => v.to_string(),
        None => {
            let list = versions(src, &p.id, None).await?;
            pick_update(&list, "release")
                .or_else(|| pick_update(&list, "alpha"))
                .map(|v| v.id.clone())
                .ok_or_else(|| unsupported("a Modrinth modpack with no versions"))?
        }
    };
    let desired = resolve(src, &p.id, &version_id).await?;
    Ok((SourceRef::Modrinth { project_id: p.id, version_id }, desired))
}

/// Is there a newer version for this Minecraft, at least as stable as the installed one?
pub async fn check(src: &Sources, project_id: &str, installed_version_id: &str, minecraft: &str) -> UpdateCheck {
    let failed = |e: LauncherError| {
        UpdateCheck::Unavailable(
            LauncherError::network(ErrorCode::UpdateCheckFailed, "Couldn't check for updates")
                .with_fix("You can still play the version you have")
                .with_detail(e.message),
        )
    };
    let installed = match version(src, installed_version_id).await {
        Ok(v) => v,
        Err(e) => return failed(e),
    };
    let list = match versions(src, project_id, Some(minecraft)).await {
        Ok(list) => list,
        Err(e) => return failed(e),
    };
    match pick_update(&list, &installed.version_type) {
        Some(v) if v.id != installed.id && v.date_published > installed.date_published => UpdateCheck::Available {
            source: SourceRef::Modrinth { project_id: project_id.to_string(), version_id: v.id.clone() },
            stamp: v.id.clone(),
            version: v.version_number.clone(),
        },
        _ => UpdateCheck::UpToDate,
    }
}
```

The facade methods in `sources/mod.rs`, plus `pub mod modrinth;`:

```rust
pub(crate) fn no_pack() -> LauncherError {
    LauncherError::pack(ErrorCode::NoPack, "This instance doesn't come from a pack")
}

impl Sources {
    /// Turn what the player handed over into a source and the pack it currently describes.
    pub async fn open(&self, input: &PackInput) -> Result<(SourceRef, DesiredState)> {
        match input {
            PackInput::Packwiz { url } => Ok((
                SourceRef::Packwiz { url: url.clone() },
                packwiz::resolve(self, url).await?,
            )),
            PackInput::MrpackFile { path } => mrpack::import_file(self, path).await,
            PackInput::Modrinth { project, version } => {
                modrinth::open(self, project, version.as_deref()).await
            }
        }
    }

    /// Exactly what `source` names (for Modrinth, that version).
    pub async fn resolve(&self, source: &SourceRef) -> Result<DesiredState> {
        match source {
            SourceRef::None => Err(no_pack()),
            SourceRef::Packwiz { url } => packwiz::resolve(self, url).await,
            SourceRef::Mrpack { sha1, .. } => mrpack::resolve_cached(self, sha1).await,
            SourceRef::Modrinth { project_id, version_id } => {
                modrinth::resolve(self, project_id, version_id).await
            }
        }
    }

    /// The cheap update check against what is installed (`installed.stamp` is the installed
    /// Modrinth version id), never against `source`, which may already name a newer version
    /// whose sync didn't finish. A local `.mrpack` never updates; importing a newer file does.
    pub async fn check(&self, source: &SourceRef, installed: &DesiredState) -> UpdateCheck {
        match source {
            SourceRef::None | SourceRef::Mrpack { .. } => UpdateCheck::UpToDate,
            SourceRef::Packwiz { url } => packwiz::check(self, url, &installed.stamp).await,
            SourceRef::Modrinth { project_id, .. } => {
                modrinth::check(self, project_id, &installed.stamp, &installed.minecraft).await
            }
        }
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --test modrinth && cargo test -p launcher-core --lib sources::modrinth`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/sources crates/launcher-core/tests/modrinth.rs
git commit -m "feat(core): Modrinth modpacks and one Sources facade for open, resolve and check

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: The lockfile, the installed-pack snapshot, and "Your additions"

**Files:**
- Create: `crates/launcher-core/src/sync/mod.rs` (only `pub mod lockfile;` for now), `crates/launcher-core/src/sync/lockfile.rs`
- Modify: `crates/launcher-core/src/lib.rs` (`pub mod sync;`)

**Interfaces:**
- Produces, in `launcher_core::sync::lockfile`:
  - `MANAGED_FILE = "managed.json"` and `PACK_FILE = "pack.json"`.
  - `ManagedFile { name: String, sha1: String, size: u64, mode: FileMode, declared: Hashes }`. `sha1` is that of the bytes written; an empty `sha1` marks a player's file that sync chose to keep.
  - `Managed { version: u32, stamp: String, pack_version: String, choices: BTreeMap<String, bool>, files: BTreeMap<String, ManagedFile> }`, with `Managed::VERSION = 1`.
  - `Managed::load(perland_dir) -> Option<Managed>` (async): `None` if missing, unreadable or a newer version.
  - `Managed::save(&self, perland_dir) -> Result<()>` (async).
  - `load_pack(perland_dir) -> Option<DesiredState>` (async) and `save_pack(perland_dir, &DesiredState) -> Result<()>` (async).
  - `additions(game_dir, managed: Option<&Managed>) -> Result<Vec<String>>` (async): file names in `mods/` (`*.jar`, `*.jar.disabled`) whose `mods/<name without .disabled>` isn't managed, sorted case-insensitively.

- [ ] **Step 1: Write the failing tests** (`lockfile.rs` `mod tests`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{PackAbout, FileMode};

    fn managed() -> Managed {
        let mut files = BTreeMap::new();
        files.insert(
            "mods/a.jar".to_string(),
            ManagedFile {
                name: "A".into(),
                sha1: "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into(),
                size: 5,
                mode: FileMode::Replace,
                declared: Hashes::sha1("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"),
            },
        );
        Managed {
            version: Managed::VERSION,
            stamp: "s1".into(),
            pack_version: "1.0.3".into(),
            choices: BTreeMap::from([("mods/opt.jar".to_string(), false)]),
            files,
        }
    }

    #[tokio::test]
    async fn managed_round_trips_and_tolerates_damage() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Managed::load(dir.path()).await, None);
        managed().save(dir.path()).await.unwrap();
        assert_eq!(Managed::load(dir.path()).await, Some(managed()));
        let raw = std::fs::read_to_string(dir.path().join(MANAGED_FILE)).unwrap();
        assert!(raw.contains("\"packVersion\": \"1.0.3\""), "{raw}");
        std::fs::write(dir.path().join(MANAGED_FILE), "{oops").unwrap();
        assert_eq!(Managed::load(dir.path()).await, None);
        let mut future = managed();
        future.version = 99;
        future.save(dir.path()).await.unwrap();
        assert_eq!(Managed::load(dir.path()).await, None, "a newer launcher's lockfile isn't guessed at");

        let mut tampered = managed();
        let entry = tampered.files["mods/a.jar"].clone();
        tampered.files.insert("../../outside.txt".into(), entry.clone());
        tampered.files.insert("mods\\b.jar".into(), entry);
        tampered.save(dir.path()).await.unwrap();
        let loaded = Managed::load(dir.path()).await.unwrap();
        assert_eq!(loaded.files.keys().collect::<Vec<_>>(), vec!["mods/a.jar"]);
    }

    #[tokio::test]
    async fn pack_snapshot_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_pack(dir.path()).await, None);
        let desired = DesiredState {
            name: "Asalith Fields".into(),
            pack_version: "1.0.3".into(),
            minecraft: "1.20.1".into(),
            loader: None,
            files: vec![],
            stamp: "abc".into(),
            about: PackAbout::default(),
            archive: None,
        };
        save_pack(dir.path(), &desired).await.unwrap();
        assert_eq!(load_pack(dir.path()).await, Some(desired));
    }

    #[tokio::test]
    async fn additions_are_unmanaged_jars_in_mods() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        for f in ["a.jar", "Mine.jar", "old.jar.disabled", "notes.txt"] {
            std::fs::write(mods.join(f), "x").unwrap();
        }
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("config/c.jar"), "x").unwrap();
        let found = additions(dir.path(), Some(&managed())).await.unwrap();
        assert_eq!(found, vec!["Mine.jar", "old.jar.disabled"]);
        let mut all = additions(dir.path(), None).await.unwrap();
        all.sort();
        assert_eq!(all, vec!["Mine.jar", "a.jar", "old.jar.disabled"]);
        let empty = tempfile::tempdir().unwrap();
        assert!(additions(empty.path(), None).await.unwrap().is_empty());
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib sync::lockfile`
Expected: compile error, no module `sync`.

- [ ] **Step 3: Implement** `crates/launcher-core/src/sync/lockfile.rs`

```rust
//! The two files sync keeps in `.perland/`: `pack.json`, the resolved pack that is installed
//! (so a pinned or offline instance can repair itself and optional toggles need no network),
//! and `managed.json`, what sync wrote. `managed.json` is always written last (spec §3.3).
use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    download::Hashes,
    error::{ErrorCode, LauncherError, Result},
    fsutil::{normalize_rel, write_atomic},
    sources::{DesiredState, FileMode},
};

pub const MANAGED_FILE: &str = "managed.json";
pub const PACK_FILE: &str = "pack.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedFile {
    pub name: String,
    /// The sha1 of the bytes sync wrote. Empty for a player's file that sync kept in place:
    /// it never matches, so the file stays the player's.
    pub sha1: String,
    pub size: u64,
    pub mode: FileMode,
    /// The hashes the pack declared, for changelogs ("updated" means these changed).
    pub declared: Hashes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Managed {
    pub version: u32,
    pub stamp: String,
    pub pack_version: String,
    /// The optional-file choices this state was built with.
    #[serde(default)]
    pub choices: BTreeMap<String, bool>,
    #[serde(default)]
    pub files: BTreeMap<String, ManagedFile>,
}

fn save_error(item: &str, e: impl std::fmt::Display) -> LauncherError {
    LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save the pack's state")
        .with_param("item", item)
        .with_detail(e)
}

impl Managed {
    pub const VERSION: u32 = 1;

    /// `None` when there is no lockfile yet, or it can't be read. Sync then treats every file
    /// on disk that the pack doesn't list as the player's, and rewrites the lockfile. Entries
    /// whose path isn't canonical are dropped: sync moves lockfile paths to the trash, so a
    /// hand-edited `../` must never reach it.
    pub async fn load(perland_dir: &Path) -> Option<Managed> {
        let bytes = tokio::fs::read(perland_dir.join(MANAGED_FILE)).await.ok()?;
        let mut managed: Managed = serde_json::from_slice(&bytes).ok()?;
        if managed.version != Self::VERSION {
            return None;
        }
        managed
            .files
            .retain(|path, _| normalize_rel(path).as_deref() == Some(path.as_str()));
        Some(managed)
    }

    pub async fn save(&self, perland_dir: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| save_error("managed", e))?;
        write_atomic(&perland_dir.join(MANAGED_FILE), &bytes).await
    }
}

pub async fn load_pack(perland_dir: &Path) -> Option<DesiredState> {
    let bytes = tokio::fs::read(perland_dir.join(PACK_FILE)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub async fn save_pack(perland_dir: &Path, desired: &DesiredState) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(desired).map_err(|e| save_error("pack", e))?;
    write_atomic(&perland_dir.join(PACK_FILE), &bytes).await
}

/// "Your additions": jars in `mods/` that sync doesn't manage. Sync never touches these.
pub async fn additions(game_dir: &Path, managed: Option<&Managed>) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut entries = match tokio::fs::read_dir(game_dir.join("mods")).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".jar") || lower.ends_with(".jar.disabled")) {
            continue;
        }
        let enabled_name = name.strip_suffix(".disabled").unwrap_or(&name);
        let key = format!("mods/{enabled_name}");
        if !managed.is_some_and(|m| m.files.contains_key(&key)) {
            out.push(name);
        }
    }
    out.sort_by_key(|n| n.to_lowercase());
    Ok(out)
}
```

`crates/launcher-core/src/sync/mod.rs`:

```rust
//! Bringing an instance's files in line with its pack (core spec §3).
pub mod lockfile;
```

`lib.rs`: add `pub mod sync;`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --lib sync::lockfile`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/lib.rs crates/launcher-core/src/sync
git commit -m "feat(core): managed.json lockfile, installed pack snapshot, and Your additions

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: Sync planning, the pure decision table

**Files:**
- Create: `crates/launcher-core/src/sync/plan.rs`
- Modify: `crates/launcher-core/src/sync/mod.rs` (`pub mod plan;`)

**Interfaces:**
- Consumes: `DesiredState`, `PackFile`, `FileMode` (Task 7); `Managed`, `ManagedFile` (Task 12).
- Produces, in `launcher_core::sync::plan`:
  - `enum OnDisk { Missing, Same { sha1, size }, Different { sha1, size } }`
  - `struct Planned { file: PackFile, replaces: bool }`
  - `struct Kept { file: PackFile, sha1: String, size: u64 }`: `sha1` is what the lockfile records.
  - `struct Plan { write: Vec<Planned>, keep: Vec<Kept>, trash: Vec<String>, kept_modified: Vec<String> }`
  - `struct Changes { from_version: Option<String>, to_version: String, added, updated, removed: Vec<String> }`. It derives `Serialize` (camelCase) and has `is_empty()`.
  - `effective_choices(&DesiredState, choices: &BTreeMap<String, bool>) -> BTreeMap<String, bool>`
  - `wanted<'a>(&'a DesiredState, choices) -> Vec<&'a PackFile>`
  - `plan(wanted: &[&PackFile], managed: Option<&Managed>, disk: &BTreeMap<String, OnDisk>) -> Plan`
  - `changes(wanted: &[&PackFile], managed: Option<&Managed>, to_version: &str) -> Changes`

The decision table (core spec §3.2 plus the `.mrpack` override rule):

| Wanted file on disk | Mode | Lockfile | Result |
|---|---|---|---|
| missing | any | any | `write` (replaces: false) |
| same hash | any | any | `keep` (disk sha1) |
| different | `Replace` | any | `write` (replaces: true) |
| different | `Preserve` | any | `keep` (disk sha1), not reported |
| different | `ReplaceUnlessModified` | entry with sha1 == disk sha1 | `write` (replaces: true) |
| different | `ReplaceUnlessModified` | entry with another or empty sha1 | `keep` (the entry's sha1), reported in `kept_modified` if the pack's hashes changed |
| different | `ReplaceUnlessModified` | no entry | `keep` (empty sha1: the player's), reported |
| present | — (not wanted) | entry | `trash` |
| present | — (not wanted) | no entry | untouched: the player's file |

- [ ] **Step 1: Write the failing tests** (the bottom of `plan.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        download::Hashes,
        sources::{FileSource, OptionalFile, PackAbout, Side},
        sync::lockfile::ManagedFile,
    };

    fn pf(path: &str, mode: FileMode, hash: &str) -> PackFile {
        PackFile {
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            source: FileSource::Remote { urls: vec![] },
            hashes: Hashes::sha1(hash),
            size: Some(1),
            side: Side::Both,
            optional: None,
            mode,
            manual: None,
        }
    }

    fn entry(name: &str, sha1: &str, declared: &str) -> ManagedFile {
        ManagedFile {
            name: name.into(),
            sha1: sha1.into(),
            size: 1,
            mode: FileMode::Replace,
            declared: Hashes::sha1(declared),
        }
    }

    fn managed(entries: &[(&str, ManagedFile)]) -> Managed {
        Managed {
            version: Managed::VERSION,
            stamp: "old".into(),
            pack_version: "1.0".into(),
            choices: BTreeMap::new(),
            files: entries.iter().map(|(p, e)| (p.to_string(), e.clone())).collect(),
        }
    }

    fn disk(entries: &[(&str, OnDisk)]) -> BTreeMap<String, OnDisk> {
        entries.iter().map(|(p, d)| (p.to_string(), d.clone())).collect()
    }

    fn diff(sha1: &str) -> OnDisk {
        OnDisk::Different { sha1: sha1.into(), size: 1 }
    }

    fn same(sha1: &str) -> OnDisk {
        OnDisk::Same { sha1: sha1.into(), size: 1 }
    }

    #[test]
    fn missing_same_and_different_files() {
        let a = pf("mods/a.jar", FileMode::Replace, "aa");
        let b = pf("mods/b.jar", FileMode::Replace, "bb");
        let c = pf("mods/c.jar", FileMode::Replace, "cc");
        let p = plan(
            &[&a, &b, &c],
            None,
            &disk(&[("mods/b.jar", same("bb")), ("mods/c.jar", diff("xx"))]),
        );
        assert_eq!(p.write, vec![
            Planned { file: a.clone(), replaces: false },
            Planned { file: c.clone(), replaces: true },
        ]);
        assert_eq!(p.keep, vec![Kept { file: b.clone(), sha1: "bb".into(), size: 1 }]);
        assert!(p.trash.is_empty() && p.kept_modified.is_empty());
    }

    #[test]
    fn preserved_files_are_only_written_when_absent() {
        let cfg = pf("config/x.toml", FileMode::Preserve, "11");
        let p = plan(&[&cfg], None, &disk(&[("config/x.toml", diff("player"))]));
        assert!(p.write.is_empty());
        assert_eq!(p.keep, vec![Kept { file: cfg.clone(), sha1: "player".into(), size: 1 }]);
        assert!(p.kept_modified.is_empty(), "preserve is silent");
        let p = plan(&[&cfg], None, &disk(&[]));
        assert_eq!(p.write, vec![Planned { file: cfg, replaces: false }]);
    }

    #[test]
    fn dropped_managed_files_go_to_the_trash_and_player_files_stay() {
        let lock = managed(&[
            ("mods/gone.jar", entry("Gone", "g1", "g1")),
            ("mods/already-deleted.jar", entry("Deleted", "d1", "d1")),
        ]);
        let p = plan(
            &[],
            Some(&lock),
            &disk(&[
                ("mods/gone.jar", diff("g1")),
                ("mods/already-deleted.jar", OnDisk::Missing),
                ("mods/mine.jar", diff("m1")),
            ]),
        );
        assert_eq!(p.trash, vec!["mods/gone.jar".to_string()]);
        assert!(p.write.is_empty() && p.keep.is_empty());
    }

    #[test]
    fn mrpack_overrides_update_only_while_unmodified() {
        let mut cfg = pf("config/c.toml", FileMode::ReplaceUnlessModified, "new");
        // The player never touched it: the disk still holds what sync wrote.
        let lock = managed(&[("config/c.toml", entry("c.toml", "written", "old"))]);
        let p = plan(&[&cfg], Some(&lock), &disk(&[("config/c.toml", diff("written"))]));
        assert_eq!(p.write, vec![Planned { file: cfg.clone(), replaces: true }]);

        // The player edited it: kept, reported, and the lockfile keeps the old sha1.
        let p = plan(&[&cfg], Some(&lock), &disk(&[("config/c.toml", diff("edited"))]));
        assert!(p.write.is_empty());
        assert_eq!(p.keep, vec![Kept { file: cfg.clone(), sha1: "written".into(), size: 1 }]);
        assert_eq!(p.kept_modified, vec!["config/c.toml".to_string()]);

        // A file the launcher never wrote is the player's.
        let p = plan(&[&cfg], None, &disk(&[("config/c.toml", diff("theirs"))]));
        assert_eq!(p.keep, vec![Kept { file: cfg.clone(), sha1: String::new(), size: 1 }]);
        assert_eq!(p.kept_modified, vec!["config/c.toml".to_string()]);

        // Still the player's next time, and not reported again while the pack's copy is unchanged.
        cfg.hashes = Hashes::sha1("old");
        let theirs = managed(&[("config/c.toml", entry("c.toml", "", "old"))]);
        let p = plan(&[&cfg], Some(&theirs), &disk(&[("config/c.toml", diff("theirs"))]));
        assert_eq!(p.keep[0].sha1, "");
        assert!(p.kept_modified.is_empty());
    }

    fn state(files: Vec<PackFile>) -> DesiredState {
        DesiredState {
            name: "P".into(),
            pack_version: "2.0".into(),
            minecraft: "1.20.1".into(),
            loader: None,
            files,
            stamp: "new".into(),
            about: PackAbout::default(),
            archive: None,
        }
    }

    #[test]
    fn only_client_files_and_chosen_optionals_are_wanted() {
        let both = pf("mods/both.jar", FileMode::Replace, "1");
        let server = PackFile { side: Side::Server, ..pf("mods/server.jar", FileMode::Replace, "2") };
        let off = PackFile {
            optional: Some(OptionalFile { default: false, description: None }),
            ..pf("mods/off.jar", FileMode::Replace, "3")
        };
        let on = PackFile {
            optional: Some(OptionalFile { default: true, description: None }),
            ..pf("mods/on.jar", FileMode::Replace, "4")
        };
        let d = state(vec![both, server, off, on]);
        let paths = |choices: &BTreeMap<String, bool>| {
            wanted(&d, choices).iter().map(|f| f.path.clone()).collect::<Vec<_>>()
        };
        assert_eq!(paths(&BTreeMap::new()), vec!["mods/both.jar", "mods/on.jar"]);
        let flipped = BTreeMap::from([
            ("mods/off.jar".to_string(), true),
            ("mods/on.jar".to_string(), false),
        ]);
        assert_eq!(paths(&flipped), vec!["mods/both.jar", "mods/off.jar"]);
        assert_eq!(
            effective_choices(&d, &BTreeMap::new()),
            BTreeMap::from([("mods/off.jar".to_string(), false), ("mods/on.jar".to_string(), true)])
        );
    }

    #[test]
    fn changes_name_what_an_update_does() {
        let mut apple2 = pf("mods/apple-2.jar", FileMode::Replace, "a2");
        apple2.name = "AppleSkin".into();
        let mut jei = pf("mods/jei.jar", FileMode::Replace, "j2");
        jei.name = "JEI".into();
        let mut fresh = pf("mods/fresh.jar", FileMode::Replace, "f1");
        fresh.name = "Fresh".into();
        let keep = pf("mods/keep.jar", FileMode::Replace, "k1");
        let lock = managed(&[
            ("mods/apple-1.jar", entry("AppleSkin", "a1", "a1")),
            ("mods/jei.jar", entry("JEI", "j1", "j1")),
            ("mods/gone.jar", entry("Gone", "g1", "g1")),
            ("mods/keep.jar", entry("keep.jar", "k1", "k1")),
        ]);
        let c = changes(&[&apple2, &jei, &fresh, &keep], Some(&lock), "2.0");
        assert_eq!(c.from_version.as_deref(), Some("1.0"));
        assert_eq!(c.to_version, "2.0");
        assert_eq!(c.added, vec!["Fresh"]);
        assert_eq!(c.updated, vec!["AppleSkin", "JEI"], "a renamed jar of the same mod is an update");
        assert_eq!(c.removed, vec!["Gone"]);
        let first = changes(&[&fresh], None, "2.0");
        assert_eq!((first.from_version, first.added), (None, vec!["Fresh".to_string()]));
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib sync::plan`
Expected: compile error, no module `plan`.

- [ ] **Step 3: Implement** `crates/launcher-core/src/sync/plan.rs` (and `pub mod plan;`)

```rust
//! The decision table of core spec §3.2, as a pure function over what is on disk.
use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::Serialize;

use crate::{
    sources::{DesiredState, FileMode, PackFile},
    sync::lockfile::Managed,
};

/// What the survey found at a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnDisk {
    Missing,
    /// Present and matching the pack's hashes.
    Same { sha1: String, size: u64 },
    /// Present with other content. For paths the pack no longer lists: simply present.
    Different { sha1: String, size: u64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub file: PackFile,
    /// A file is in the way; it goes to the trash before the new one moves in.
    pub replaces: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Kept {
    pub file: PackFile,
    /// What the lockfile records for it (empty: the player's file).
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub write: Vec<Planned>,
    pub keep: Vec<Kept>,
    /// Managed files the pack no longer wants that are still on disk.
    pub trash: Vec<String>,
    /// `.mrpack` override files the player changed, kept instead of updated.
    pub kept_modified: Vec<String>,
}

/// What an update does, in names a player recognizes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub from_version: Option<String>,
    pub to_version: String,
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.updated.is_empty() && self.removed.is_empty()
    }
}

/// The choice for every optional client file: the player's, else the pack's default.
pub fn effective_choices(
    desired: &DesiredState,
    choices: &BTreeMap<String, bool>,
) -> BTreeMap<String, bool> {
    desired
        .files
        .iter()
        .filter(|f| f.side.on_client())
        .filter_map(|f| {
            let opt = f.optional.as_ref()?;
            Some((f.path.clone(), choices.get(&f.path).copied().unwrap_or(opt.default)))
        })
        .collect()
}

/// The files this client should have: client and both-side ones, minus optional files turned off.
pub fn wanted<'a>(desired: &'a DesiredState, choices: &BTreeMap<String, bool>) -> Vec<&'a PackFile> {
    let effective = effective_choices(desired, choices);
    desired
        .files
        .iter()
        .filter(|f| f.side.on_client())
        .filter(|f| effective.get(&f.path).copied().unwrap_or(true))
        .collect()
}

pub fn plan(wanted: &[&PackFile], managed: Option<&Managed>, disk: &BTreeMap<String, OnDisk>) -> Plan {
    let mut plan = Plan::default();
    let lock = |path: &str| managed.and_then(|m| m.files.get(path));
    for &file in wanted {
        let keep = |sha1: &str, size: u64| Kept { file: file.clone(), sha1: sha1.to_string(), size };
        match disk.get(&file.path).unwrap_or(&OnDisk::Missing) {
            OnDisk::Missing => plan.write.push(Planned { file: file.clone(), replaces: false }),
            OnDisk::Same { sha1, size } => plan.keep.push(keep(sha1, *size)),
            OnDisk::Different { sha1, size } => match file.mode {
                FileMode::Replace => plan.write.push(Planned { file: file.clone(), replaces: true }),
                FileMode::Preserve => plan.keep.push(keep(sha1, *size)),
                FileMode::ReplaceUnlessModified => match lock(&file.path) {
                    Some(entry) if !entry.sha1.is_empty() && entry.sha1 == *sha1 => {
                        plan.write.push(Planned { file: file.clone(), replaces: true })
                    }
                    Some(entry) => {
                        plan.keep.push(keep(&entry.sha1, entry.size));
                        if entry.declared != file.hashes {
                            plan.kept_modified.push(file.path.clone());
                        }
                    }
                    None => {
                        plan.keep.push(keep("", *size));
                        plan.kept_modified.push(file.path.clone());
                    }
                },
            },
        }
    }
    if let Some(m) = managed {
        let wanted_paths: HashSet<&str> = wanted.iter().map(|f| f.path.as_str()).collect();
        for path in m.files.keys() {
            let present = !matches!(disk.get(path), None | Some(OnDisk::Missing));
            if present && !wanted_paths.contains(path.as_str()) {
                plan.trash.push(path.clone());
            }
        }
    }
    plan
}

/// The changelog for syncing to `wanted`. A mod whose jar was renamed (same name, new path)
/// counts as updated, not as removed plus added.
pub fn changes(wanted: &[&PackFile], managed: Option<&Managed>, to_version: &str) -> Changes {
    let mut added = BTreeSet::new();
    let mut updated = BTreeSet::new();
    let mut removed = BTreeSet::new();
    let wanted_paths: HashSet<&str> = wanted.iter().map(|f| f.path.as_str()).collect();
    for f in wanted {
        match managed.and_then(|m| m.files.get(&f.path)) {
            None => {
                added.insert(f.name.clone());
            }
            Some(entry) if entry.declared != f.hashes => {
                updated.insert(f.name.clone());
            }
            Some(_) => {}
        }
    }
    if let Some(m) = managed {
        for (path, entry) in &m.files {
            if !wanted_paths.contains(path.as_str()) {
                removed.insert(entry.name.clone());
            }
        }
    }
    let renamed: Vec<String> = added.intersection(&removed).cloned().collect();
    for name in renamed {
        added.remove(&name);
        removed.remove(&name);
        updated.insert(name);
    }
    Changes {
        from_version: managed.map(|m| m.pack_version.clone()),
        to_version: to_version.to_string(),
        added: added.into_iter().collect(),
        updated: updated.into_iter().collect(),
        removed: removed.into_iter().collect(),
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --lib sync::plan`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/sync
git commit -m "feat(core): the sync decision table, optional choices and changelog

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: Applying a sync: survey, free space, trash, moves

**Files:**
- Create: `crates/launcher-core/src/sync/apply.rs`
- Modify: `crates/launcher-core/src/sync/mod.rs` (`pub(crate) mod apply;`)
- Modify: `Cargo.toml` (workspace) and `crates/launcher-core/Cargo.toml`: add `fs4 = "1.1"`

**Interfaces:**
- Consumes: `OnDisk` (Task 13), `Managed` (Task 12), `sha1_file`, `Hashes::verify_file`.
- Produces, `pub(crate)` in `sync::apply`:
  - `TRASH_MAX_AGE: Duration` (7 days) and `SPACE_MARGIN: u64` (100 MiB).
  - `purge_trash(trash: &Path, now: SystemTime, max_age: Duration)` (async): best effort. Folders are named by unix seconds.
  - `trash_folder(perland_dir: &Path, now: SystemTime) -> PathBuf`
  - `quick_check(game_dir: &Path, managed: &Managed) -> bool`: every managed file exists, and `Replace` files kept their size.
  - `survey(game_dir, wanted: &[&PackFile], managed: Option<&Managed>) -> Result<BTreeMap<String, OnDisk>>` (async): hashes every wanted and lockfile path.
  - `ensure_space(needed: u64, free: u64) -> Result<()>`: `NotEnoughSpace` with `needed_mb`/`free_mb`, where needed includes the margin.
  - `free_space(path: &Path) -> u64`: `u64::MAX` when unknown, so an unknown never blocks.
  - `move_to_trash(game_dir, trash_dir, rel) -> Result<()>` (async): a missing file is fine.
  - `place(staged: &Path, target: &Path) -> Result<()>` (async)

- [ ] **Step 1: Write the failing tests** (the bottom of `apply.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        download::hash::{sha1_hex, sha256_hex},
        sources::{FileMode, FileSource, Side},
        sync::lockfile::ManagedFile,
        ErrorCode,
    };
    use std::collections::BTreeMap;

    fn pf(path: &str, hashes: Hashes) -> PackFile {
        PackFile {
            path: path.into(),
            name: path.into(),
            source: FileSource::Remote { urls: vec![] },
            hashes,
            size: None,
            side: Side::Both,
            optional: None,
            mode: FileMode::Replace,
            manual: None,
        }
    }

    fn managed(files: &[(&str, FileMode, u64)]) -> Managed {
        Managed {
            version: Managed::VERSION,
            stamp: "s".into(),
            pack_version: "1".into(),
            choices: BTreeMap::new(),
            files: files
                .iter()
                .map(|(p, mode, size)| {
                    (
                        p.to_string(),
                        ManagedFile {
                            name: p.to_string(),
                            sha1: "x".into(),
                            size: *size,
                            mode: *mode,
                            declared: Hashes::sha1("x"),
                        },
                    )
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn old_trash_is_purged_and_recent_trash_kept() {
        let dir = tempfile::tempdir().unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 86_400);
        for name in ["86400", "860000", "notes"] {
            std::fs::create_dir_all(dir.path().join(name).join("mods")).unwrap();
        }
        purge_trash(dir.path(), now, TRASH_MAX_AGE).await;
        assert!(!dir.path().join("86400").exists(), "9 days old");
        assert!(dir.path().join("860000").exists(), "under a day old");
        assert!(dir.path().join("notes").exists(), "not ours to delete");
        purge_trash(&dir.path().join("missing"), now, TRASH_MAX_AGE).await;
        assert_eq!(
            trash_folder(Path::new("p"), now),
            Path::new("p").join("trash").join("864000")
        );
    }

    #[test]
    fn quick_check_needs_every_file_and_exact_sizes_for_pack_owned_ones() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("mods")).unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("mods/a.jar"), "12345").unwrap();
        std::fs::write(dir.path().join("config/c.toml"), "edited by the player").unwrap();
        let ok = managed(&[("mods/a.jar", FileMode::Replace, 5), ("config/c.toml", FileMode::Preserve, 3)]);
        assert!(quick_check(dir.path(), &ok));
        let resized = managed(&[("mods/a.jar", FileMode::Replace, 6)]);
        assert!(!quick_check(dir.path(), &resized));
        let missing = managed(&[("mods/b.jar", FileMode::Replace, 5)]);
        assert!(!quick_check(dir.path(), &missing));
    }

    #[tokio::test]
    async fn survey_hashes_what_is_there() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("mods")).unwrap();
        std::fs::write(dir.path().join("mods/a.jar"), "hello").unwrap();
        std::fs::write(dir.path().join("mods/b.jar"), "jello").unwrap();
        std::fs::write(dir.path().join("mods/old.jar"), "old").unwrap();
        let a = pf("mods/a.jar", Hashes::sha1(sha1_hex(b"hello")));
        let b = pf(
            "mods/b.jar",
            Hashes { sha256: Some(sha256_hex(b"hello")), ..Hashes::default() },
        );
        let c = pf("mods/c.jar", Hashes::sha1(sha1_hex(b"c")));
        let lock = managed(&[("mods/old.jar", FileMode::Replace, 3)]);
        let found = survey(dir.path(), &[&a, &b, &c], Some(&lock)).await.unwrap();
        assert_eq!(found["mods/a.jar"], OnDisk::Same { sha1: sha1_hex(b"hello"), size: 5 });
        assert_eq!(found["mods/b.jar"], OnDisk::Different { sha1: sha1_hex(b"jello"), size: 5 });
        assert_eq!(found["mods/c.jar"], OnDisk::Missing);
        assert!(matches!(found["mods/old.jar"], OnDisk::Different { .. }), "present");
    }

    #[test]
    fn free_space_is_checked_with_a_margin() {
        ensure_space(1 << 20, 200 << 20).unwrap();
        let err = ensure_space(1 << 30, 50 << 20).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotEnoughSpace);
        assert_eq!(err.params["needed_mb"], "1124");
        assert_eq!(err.params["free_mb"], "50");
        assert!(free_space(Path::new(".")) > 0);
    }

    #[tokio::test]
    async fn moves_create_folders_and_tolerate_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        let trash = dir.path().join("trash/1");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/a.jar"), "a").unwrap();
        move_to_trash(&game, &trash, "mods/a.jar").await.unwrap();
        assert!(!game.join("mods/a.jar").exists());
        assert_eq!(std::fs::read_to_string(trash.join("mods/a.jar")).unwrap(), "a");
        move_to_trash(&game, &trash, "mods/never.jar").await.unwrap();

        let staged = dir.path().join("staging/config/x.toml");
        std::fs::create_dir_all(staged.parent().unwrap()).unwrap();
        std::fs::write(&staged, "x").unwrap();
        place(&staged, &game.join("config/x.toml")).await.unwrap();
        assert_eq!(std::fs::read_to_string(game.join("config/x.toml")).unwrap(), "x");
        assert!(!staged.exists());
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib sync::apply`
Expected: compile error, no module `apply`.

- [ ] **Step 3: Implement**

Add `fs4 = "1.1"` to `[workspace.dependencies]` and `fs4.workspace = true` to `launcher-core`. Then `crates/launcher-core/src/sync/apply.rs`:

```rust
//! The file-system half of a sync: what is on disk, free space, the trash, and moving files.
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::{lockfile::Managed, plan::OnDisk};
use crate::{
    download::{hash::sha1_file, Hashes},
    error::{ErrorCode, LauncherError, Result},
    sources::{FileMode, PackFile},
};

/// Removed and replaced files stay recoverable this long (spec §3.2).
pub(crate) const TRASH_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Free space kept on top of what a sync needs.
pub(crate) const SPACE_MARGIN: u64 = 100 << 20;

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Delete trash folders (named by unix seconds) older than `max_age`. Best effort.
pub(crate) async fn purge_trash(trash: &Path, now: SystemTime, max_age: Duration) {
    let Ok(mut entries) = tokio::fs::read_dir(trash).await else {
        return;
    };
    let cutoff = unix_secs(now).saturating_sub(max_age.as_secs());
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(secs) = entry.file_name().to_string_lossy().parse::<u64>() else {
            continue;
        };
        if secs < cutoff {
            let _ = tokio::fs::remove_dir_all(entry.path()).await;
        }
    }
}

pub(crate) fn trash_folder(perland_dir: &Path, now: SystemTime) -> PathBuf {
    perland_dir.join("trash").join(unix_secs(now).to_string())
}

/// Cheap, with no hashing: every managed file still exists, and the files the pack owns outright
/// kept their size. Files the player may edit (preserved, kept overrides) only need to exist.
pub(crate) fn quick_check(game_dir: &Path, managed: &Managed) -> bool {
    managed.files.iter().all(|(path, entry)| match std::fs::metadata(game_dir.join(path)) {
        Ok(meta) => entry.mode != FileMode::Replace || meta.len() == entry.size,
        Err(_) => false,
    })
}

/// Hash what's on disk at every path the pack wants or the lockfile names.
pub(crate) async fn survey(
    game_dir: &Path,
    wanted: &[&PackFile],
    managed: Option<&Managed>,
) -> Result<BTreeMap<String, OnDisk>> {
    let mut jobs: Vec<(String, Option<Hashes>)> = wanted
        .iter()
        .map(|f| (f.path.clone(), Some(f.hashes.clone())))
        .collect();
    let mut seen: HashSet<String> = jobs.iter().map(|(p, _)| p.clone()).collect();
    if let Some(m) = managed {
        for path in m.files.keys() {
            if seen.insert(path.clone()) {
                jobs.push((path.clone(), None));
            }
        }
    }
    let game_dir = game_dir.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<BTreeMap<String, OnDisk>> {
        let mut out = BTreeMap::new();
        for (path, hashes) in jobs {
            let full = game_dir.join(&path);
            let state = match std::fs::metadata(&full) {
                Err(_) => OnDisk::Missing,
                Ok(meta) if !meta.is_file() => OnDisk::Different { sha1: String::new(), size: 0 },
                Ok(meta) => {
                    let sha1 = sha1_file(&full)?;
                    let same = match &hashes {
                        None => false,
                        Some(h) => match &h.sha1 {
                            Some(expected) => expected.eq_ignore_ascii_case(&sha1),
                            None => h.verify_file(&full)?,
                        },
                    };
                    let size = meta.len();
                    if same {
                        OnDisk::Same { sha1, size }
                    } else {
                        OnDisk::Different { sha1, size }
                    }
                }
            };
            out.insert(path, state);
        }
        Ok(out)
    })
    .await
    .map_err(|e| LauncherError::from(std::io::Error::other(e)))?
}

pub(crate) fn ensure_space(needed: u64, free: u64) -> Result<()> {
    let needed = needed.saturating_add(SPACE_MARGIN);
    if needed <= free {
        return Ok(());
    }
    let mb = |bytes: u64| bytes.div_ceil(1 << 20);
    Err(LauncherError::disk(
        ErrorCode::NotEnoughSpace,
        format!("Not enough free space: {} MB needed, {} MB free", mb(needed), mb(free)),
    )
    .with_param("needed_mb", mb(needed))
    .with_param("free_mb", mb(free))
    .with_fix("Free up some space and try again"))
}

/// Free bytes on the volume holding `path`; when that can't be read, assume plenty: a full
/// disk still fails cleanly while writing.
pub(crate) fn free_space(path: &Path) -> u64 {
    fs4::available_space(path).unwrap_or(u64::MAX)
}

/// Move `game_dir/rel` to `trash_dir/rel`. A file that's already gone is fine.
pub(crate) async fn move_to_trash(game_dir: &Path, trash_dir: &Path, rel: &str) -> Result<()> {
    let from = game_dir.join(rel);
    if tokio::fs::symlink_metadata(&from).await.is_err() {
        return Ok(());
    }
    let to = trash_dir.join(rel);
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::rename(&from, &to).await?;
    Ok(())
}

/// Move a staged file into place (same volume: the instance folder).
pub(crate) async fn place(staged: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::rename(staged, target).await?;
    Ok(())
}
```

`sync/mod.rs`: add `pub(crate) mod apply;`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --lib sync::apply`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/launcher-core/Cargo.toml crates/launcher-core/src/sync
git commit -m "feat(core): survey, free-space check, 7-day trash and file moves for sync

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: The `Syncer`: staging, the game install first, swap, lockfile last

**Files:**
- Modify: `crates/launcher-core/src/sync/mod.rs`
- Test: `crates/launcher-core/tests/sync.rs` (new; Tasks 16–18 add to it)

**Interfaces:**
- Consumes:
  - Tasks 5–14: `InstanceLock`, `ObjectCache`, `Sources`, `mrpack::extract_entry`, `plan::*`, `apply::*`, `Managed`, `save_pack`.
  - `InstanceStore::{load, save, game_dir, perland_dir}`.
- Produces, in `launcher_core::sync`:
  - `type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>`
  - `trait GameInstall: Send + Sync { fn install_game<'a>(&'a self, game: &'a GameSpec, sink: &'a EventSink) -> BoxFuture<'a, Result<()>>; }`, implemented for `Installer` (resolve + install).
  - `struct ManualFile { path, name, file_name, page_url, hashes: Hashes }`: `Serialize`, camelCase.
  - `struct SyncReport { up_to_date: bool, changes: Changes, kept_modified: Vec<String>, game_changed: bool }`: `Serialize`, camelCase, `Default`.
  - `enum SyncOutcome { Done(SyncReport), NeedsManual(Vec<ManualFile>) }`
  - `struct Syncer { pub game: Arc<dyn GameInstall>, pub sources: Sources, pub root: PathBuf }`, with:
    - `new(game, sources, root)`
    - `for_installer(installer: Arc<Installer>, http: reqwest::Client)`: the production wiring, with the cache at `dirs.objects()` and the real addresses.
    - `sync(&self, store, id, desired, sink) -> Result<SyncOutcome>`: takes the instance lock; `InstanceLocked` if it's held.
    - `sync_locked(&self, lock: &InstanceLock, store, id, desired, sink) -> Result<SyncOutcome>`
- Emits progress task key `sync_pack` with param `name`.

**Order inside `sync_locked`** (spec §3.3):
1. Purge old trash. Check paths.
2. Fast path.
3. Survey, plan, check free space.
4. Stage every write in `.perland/staging/` (from the cache, or the archive). Blocked CurseForge files mean `NeedsManual`, and nothing is applied.
5. If the game changed, install it. A failure returns the error with nothing applied.
6. Move replaced and dropped files to the trash, and staged files into place.
7. Save `instance.json` (game).
8. Write `pack.json`, then `managed.json` last.
9. Remove the staging folder.

- [ ] **Step 1: Write the failing tests** (`crates/launcher-core/tests/sync.rs`)

```rust
mod common;

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use common::{hits, sources, PackBuilder};
use launcher_core::{
    error::{LauncherError, Result},
    events::{Event, EventSink},
    instances::{GameSpec, Instance, InstanceStore, LoaderSpec},
    locks::InstanceLock,
    meta::loaders::LoaderKind,
    sources::{DesiredState, PackInput},
    sync::{lockfile::Managed, BoxFuture, GameInstall, SyncOutcome, SyncReport, Syncer},
    ErrorCode,
};
use wiremock::MockServer;

/// Records installs instead of installing; `fail` makes them fail.
#[derive(Default)]
struct FakeGame {
    installs: Mutex<Vec<GameSpec>>,
    fail: AtomicBool,
}

impl GameInstall for FakeGame {
    fn install_game<'a>(&'a self, game: &'a GameSpec, _sink: &'a EventSink) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.installs.lock().unwrap().push(game.clone());
            if self.fail.load(Ordering::SeqCst) {
                Err(LauncherError::loader(ErrorCode::LoaderVersionNotFound, "no such Forge"))
            } else {
                Ok(())
            }
        })
    }
}

struct Env {
    server: MockServer,
    _tmp: tempfile::TempDir,
    store: InstanceStore,
    syncer: Syncer,
    game: Arc<FakeGame>,
    events: Arc<Mutex<Vec<Event>>>,
}

impl Env {
    async fn new() -> Self {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path().join("instances"));
        let game = Arc::new(FakeGame::default());
        let syncer = Syncer::new(game.clone(), sources(&server, tmp.path()), tmp.path());
        Self { server, _tmp: tmp, store, syncer, game, events: Arc::default() }
    }

    fn sink(&self) -> EventSink {
        let events = self.events.clone();
        Arc::new(move |e| events.lock().unwrap().push(e))
    }

    /// An instance for the pack at `url`, created the way `add_pack` (Task 17) does.
    async fn instance(&self, url: &str) -> (Instance, DesiredState) {
        let (source, desired) = self
            .syncer
            .sources
            .open(&PackInput::Packwiz { url: url.into() })
            .await
            .unwrap();
        let mut instance = self
            .store
            .create(
                &desired.name,
                GameSpec { minecraft: desired.minecraft.clone(), loader: desired.loader.clone() },
            )
            .await
            .unwrap();
        instance.source = source;
        self.store.save(&instance).await.unwrap();
        (instance, desired)
    }

    fn game_file(&self, id: &str, rel: &str) -> PathBuf {
        self.store.game_dir(id).join(rel)
    }

    async fn sync(&self, id: &str, desired: &DesiredState) -> Result<SyncOutcome> {
        self.syncer.sync(&self.store, id, desired, &self.sink()).await
    }
}

fn done(outcome: SyncOutcome) -> SyncReport {
    match outcome {
        SyncOutcome::Done(report) => report,
        other => panic!("expected Done, got {other:?}"),
    }
}

fn v1(server: &MockServer) -> PackBuilder {
    PackBuilder::new(server, "Tiny", "1.0.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v1", "both", None)
        .url_mod("zoom", "zoom.jar", b"zoom", "client", None)
        .url_mod("backups", "backups.jar", b"backup", "server", None)
        .url_mod("shaders", "shaders.jar", b"shade", "client", Some(false))
        .file("config/a.toml", b"a = 1", true)
}

#[tokio::test]
async fn fresh_sync_installs_client_files_and_writes_the_lockfile_last() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    let report = done(env.sync(&instance.id, &desired).await.unwrap());

    assert!(!report.up_to_date && !report.game_changed);
    assert_eq!(report.changes.added, vec!["a.toml", "appleskin", "zoom"]);
    assert_eq!(std::fs::read(env.game_file(&instance.id, "mods/appleskin.jar")).unwrap(), b"apple v1");
    assert!(env.game_file(&instance.id, "mods/zoom.jar").exists());
    assert!(env.game_file(&instance.id, "config/a.toml").exists());
    assert!(!env.game_file(&instance.id, "mods/backups.jar").exists(), "server-only");
    assert!(!env.game_file(&instance.id, "mods/shaders.jar").exists(), "optional, off by default");

    let perland = env.store.perland_dir(&instance.id);
    let managed = Managed::load(&perland).await.unwrap();
    assert_eq!(managed.stamp, desired.stamp);
    assert_eq!(managed.files.len(), 3);
    assert!(!managed.choices["mods/shaders.jar"], "the optional default is recorded");
    assert!(perland.join("pack.json").exists());
    assert!(!perland.join("staging").exists());
    assert!(env.game.installs.lock().unwrap().is_empty(), "same game: nothing to install");
    let progress = env.events.lock().unwrap().iter().filter(|e| matches!(e,
        Event::Progress { task, .. } if task.key == "sync_pack" && task.params["name"] == "Tiny")).count();
    assert!(progress >= 2, "{progress}");
}

#[tokio::test]
async fn an_unchanged_pack_takes_the_fast_path() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    done(env.sync(&instance.id, &desired).await.unwrap());
    let report = done(env.sync(&instance.id, &desired).await.unwrap());
    assert!(report.up_to_date);
    assert_eq!(hits(&env.server, "/files/appleskin.jar").await, 1);

    // A deleted mod is noticed and restored from the cache, without downloading it again.
    std::fs::remove_file(env.game_file(&instance.id, "mods/zoom.jar")).unwrap();
    let report = done(env.sync(&instance.id, &desired).await.unwrap());
    assert!(!report.up_to_date);
    assert!(env.game_file(&instance.id, "mods/zoom.jar").exists());
    assert_eq!(hits(&env.server, "/files/zoom.jar").await, 1);
}

#[tokio::test]
async fn a_locked_instance_is_refused() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    let held = InstanceLock::try_acquire(&env.store.perland_dir(&instance.id)).unwrap();
    let err = env.sync(&instance.id, &desired).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InstanceLocked);
    assert!(!env.game_file(&instance.id, "mods/appleskin.jar").exists());
    drop(held);
}

#[tokio::test]
async fn a_new_loader_is_installed_before_any_file_moves() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    done(env.sync(&instance.id, &desired).await.unwrap());

    env.server.reset().await;
    v1(&env.server)
        .with_version("1.1.0")
        .with_loader("forge", "47.4.30")
        .url_mod("jei", "jei.jar", b"jei", "both", None)
        .mount(&env.server)
        .await;
    let v2 = env.syncer.sources.resolve(&instance.source).await.unwrap();
    let report = done(env.sync(&instance.id, &v2).await.unwrap());
    assert!(report.game_changed);
    let wanted = GameSpec {
        minecraft: "1.20.1".into(),
        loader: Some(LoaderSpec { kind: LoaderKind::Forge, version: "47.4.30".into() }),
    };
    assert_eq!(*env.game.installs.lock().unwrap(), vec![wanted.clone()]);
    assert_eq!(env.store.load(&instance.id).await.unwrap().game, wanted);
    assert!(env.game_file(&instance.id, "mods/jei.jar").exists());
}

#[tokio::test]
async fn failed_game_install_leaves_the_instance_untouched() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    done(env.sync(&instance.id, &desired).await.unwrap());

    env.server.reset().await;
    PackBuilder::new(&env.server, "Tiny", "2.0.0")
        .with_loader("forge", "99.0.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v2", "both", None)
        .mount(&env.server)
        .await;
    let v2 = env.syncer.sources.resolve(&instance.source).await.unwrap();
    env.game.fail.store(true, Ordering::SeqCst);
    let err = env.sync(&instance.id, &v2).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::LoaderVersionNotFound);

    assert_eq!(env.store.load(&instance.id).await.unwrap().game, instance.game);
    assert_eq!(std::fs::read(env.game_file(&instance.id, "mods/appleskin.jar")).unwrap(), b"apple v1");
    assert!(env.game_file(&instance.id, "mods/zoom.jar").exists(), "nothing was removed");
    let managed = Managed::load(&env.store.perland_dir(&instance.id)).await.unwrap();
    assert_eq!(managed.stamp, desired.stamp, "the old lockfile still describes the instance");
}
```

Later tasks add more tests (and imports) to this file.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --test sync`
Expected: compile errors (`Syncer`, `GameInstall`, `SyncOutcome` and `BoxFuture` not found in `sync`).

- [ ] **Step 3: Implement** in `crates/launcher-core/src/sync/mod.rs`

```rust
//! Bringing an instance's files in line with its pack (core spec §3). `sources` says what the
//! pack is; this module decides (`plan`), touches the disk (`apply`), and records (`lockfile`).
pub(crate) mod apply;
pub mod lockfile;
pub mod plan;

use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::SystemTime,
};

use futures_util::StreamExt;
use serde::Serialize;

use crate::{
    download::{cache::ObjectCache, hash::sha1_file, Hashes},
    error::{ErrorCode, LauncherError, Result},
    events::{Event, EventSink, TaskId},
    install::Installer,
    instances::{GameSpec, InstanceStore},
    locks::InstanceLock,
    sources::{mrpack, DesiredState, FileSource, PackFile, SourceConfig, Sources},
};
use lockfile::{save_pack, Managed, ManagedFile};
use plan::{Changes, Planned};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Installs a Minecraft and loader version. The real one is `Installer`; tests use a fake.
pub trait GameInstall: Send + Sync {
    fn install_game<'a>(&'a self, game: &'a GameSpec, sink: &'a EventSink) -> BoxFuture<'a, Result<()>>;
}

impl GameInstall for Installer {
    fn install_game<'a>(&'a self, game: &'a GameSpec, sink: &'a EventSink) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let loader = game.loader.as_ref().map(|l| (l.kind, l.version.as_str()));
            let resolved = self.resolve(&game.minecraft, loader).await?;
            self.install(&resolved, sink).await
        })
    }
}

/// A file its author blocks other launchers from downloading: the player gets it by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualFile {
    pub path: String,
    pub name: String,
    pub file_name: String,
    pub page_url: String,
    pub hashes: Hashes,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    /// Nothing needed doing.
    pub up_to_date: bool,
    pub changes: Changes,
    /// Override files the player changed, kept instead of updated ("Kept your version of …").
    pub kept_modified: Vec<String>,
    pub game_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    Done(SyncReport),
    /// Nothing was applied: these files must be downloaded by hand first (see `import_manual`).
    NeedsManual(Vec<ManualFile>),
}

/// How many pack files are fetched or copied at once.
const STAGE_CONCURRENCY: usize = 8;

struct StagedFile {
    sha1: String,
    size: u64,
}

enum StageError {
    Blocked(ManualFile),
    Failed(LauncherError),
}

impl From<LauncherError> for StageError {
    fn from(e: LauncherError) -> Self {
        StageError::Failed(e)
    }
}

fn archive_gone() -> LauncherError {
    LauncherError::pack(ErrorCode::PackNotFound, "The pack's archive is gone")
        .with_fix("Import the pack again")
}

fn join_error(e: tokio::task::JoinError) -> LauncherError {
    std::io::Error::other(e).into()
}

pub struct Syncer {
    pub game: Arc<dyn GameInstall>,
    pub sources: Sources,
    /// The launcher's data folder; free space is measured here.
    pub root: PathBuf,
}

impl Syncer {
    pub fn new(game: Arc<dyn GameInstall>, sources: Sources, root: impl Into<PathBuf>) -> Self {
        Self { game, sources, root: root.into() }
    }

    /// The production wiring: the installer's metadata client and downloader, the object cache
    /// under its data folder, and the real upstream addresses.
    pub fn for_installer(installer: Arc<Installer>, http: reqwest::Client) -> Self {
        let sources = Sources {
            http,
            meta: installer.meta.clone(),
            downloader: installer.downloader.clone(),
            cache: ObjectCache::new(installer.dirs.objects()),
            config: SourceConfig::default(),
        };
        let root = installer.dirs.root().to_path_buf();
        Self::new(installer, sources, root)
    }

    /// Make instance `id` match `desired`. Fails with `instance_locked` while anyone else holds
    /// the instance's lock.
    pub async fn sync(
        &self,
        store: &InstanceStore,
        id: &str,
        desired: &DesiredState,
        sink: &EventSink,
    ) -> Result<SyncOutcome> {
        let lock = InstanceLock::try_acquire(&store.perland_dir(id))?;
        self.sync_locked(&lock, store, id, desired, sink).await
    }

    /// `sync` for a caller that already holds the instance's lock.
    pub async fn sync_locked(
        &self,
        _lock: &InstanceLock,
        store: &InstanceStore,
        id: &str,
        desired: &DesiredState,
        sink: &EventSink,
    ) -> Result<SyncOutcome> {
        let mut instance = store.load(id).await?;
        let game_dir = store.game_dir(id);
        let perland = store.perland_dir(id);
        let now = SystemTime::now();
        apply::purge_trash(&perland.join("trash"), now, apply::TRASH_MAX_AGE).await;
        desired.check_paths()?;

        let managed = Managed::load(&perland).await;
        let choices = plan::effective_choices(desired, &instance.optional_choices);
        let wanted = plan::wanted(desired, &instance.optional_choices);
        let new_game = GameSpec {
            minecraft: desired.minecraft.clone(),
            loader: desired.loader.clone(),
        };
        let game_changed = instance.game != new_game;
        if let Some(m) = &managed {
            if !game_changed
                && m.stamp == desired.stamp
                && m.choices == choices
                && apply::quick_check(&game_dir, m)
            {
                return Ok(SyncOutcome::Done(SyncReport { up_to_date: true, ..SyncReport::default() }));
            }
        }

        let changes = plan::changes(&wanted, managed.as_ref(), &desired.pack_version);
        let disk = apply::survey(&game_dir, &wanted, managed.as_ref()).await?;
        let plan = plan::plan(&wanted, managed.as_ref(), &disk);

        // Staged copies of everything written, plus downloads that aren't cached yet.
        let mut needed = 0u64;
        for p in &plan.write {
            let size = p.file.size.unwrap_or(0);
            needed += size;
            let cached = self.sources.cache.path_for(&p.file.hashes).is_some_and(|o| o.exists());
            if matches!(p.file.source, FileSource::Remote { .. }) && !cached {
                needed += size;
            }
        }
        apply::ensure_space(needed, apply::free_space(&self.root))?;

        let staging = perland.join("staging");
        let _ = tokio::fs::remove_dir_all(&staging).await;
        tokio::fs::create_dir_all(&staging).await?;
        let archive = match &desired.archive {
            Some(h) => Some(self.sources.cache.get(h).await.ok_or_else(archive_gone)?),
            None => None,
        };
        let staged = match self
            .stage_all(&plan.write, archive.as_deref(), &staging, &desired.name, sink)
            .await?
        {
            Ok(staged) => staged,
            Err(blocked) => return Ok(SyncOutcome::NeedsManual(blocked)),
        };

        if game_changed {
            self.game.install_game(&new_game, sink).await?;
        }

        let trash_dir = apply::trash_folder(&perland, now);
        for p in &plan.write {
            if p.replaces {
                apply::move_to_trash(&game_dir, &trash_dir, &p.file.path).await?;
            }
            apply::place(&staging.join(&p.file.path), &game_dir.join(&p.file.path)).await?;
        }
        for path in &plan.trash {
            apply::move_to_trash(&game_dir, &trash_dir, path).await?;
        }

        if game_changed {
            instance.game = new_game;
            store.save(&instance).await?;
        }

        let mut files = BTreeMap::new();
        for k in &plan.keep {
            files.insert(k.file.path.clone(), ManagedFile {
                name: k.file.name.clone(),
                sha1: k.sha1.clone(),
                size: k.size,
                mode: k.file.mode,
                declared: k.file.hashes.clone(),
            });
        }
        for p in &plan.write {
            let s = &staged[&p.file.path];
            files.insert(p.file.path.clone(), ManagedFile {
                name: p.file.name.clone(),
                sha1: s.sha1.clone(),
                size: s.size,
                mode: p.file.mode,
                declared: p.file.hashes.clone(),
            });
        }
        save_pack(&perland, desired).await?;
        Managed {
            version: Managed::VERSION,
            stamp: desired.stamp.clone(),
            pack_version: desired.pack_version.clone(),
            choices,
            files,
        }
        .save(&perland)
        .await?;
        let _ = tokio::fs::remove_dir_all(&staging).await;
        Ok(SyncOutcome::Done(SyncReport {
            up_to_date: false,
            changes,
            kept_modified: plan.kept_modified,
            game_changed,
        }))
    }

    /// Stage every file. The outer error stops the sync; the inner `Err` lists blocked files.
    async fn stage_all(
        &self,
        writes: &[Planned],
        archive: Option<&Path>,
        staging: &Path,
        pack_name: &str,
        sink: &EventSink,
    ) -> Result<std::result::Result<BTreeMap<String, StagedFile>, Vec<ManualFile>>> {
        let task = TaskId::new("sync_pack").with("name", pack_name);
        let total = writes.len() as u64;
        let (mut done, mut bytes) = (0u64, 0u64);
        sink(Event::Progress { task: task.clone(), done, total, bytes });
        let mut results = futures_util::stream::iter(
            writes.iter().map(|p| self.stage_one(&p.file, archive, staging)),
        )
        .buffer_unordered(STAGE_CONCURRENCY);
        let mut staged = BTreeMap::new();
        let mut blocked = Vec::new();
        while let Some(result) = results.next().await {
            match result {
                Ok((path, file)) => {
                    bytes += file.size;
                    staged.insert(path, file);
                }
                Err(StageError::Blocked(m)) => blocked.push(m),
                Err(StageError::Failed(e)) => return Err(e),
            }
            done += 1;
            sink(Event::Progress { task: task.clone(), done, total, bytes });
        }
        if blocked.is_empty() {
            Ok(Ok(staged))
        } else {
            blocked.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(Err(blocked))
        }
    }

    async fn stage_one(
        &self,
        file: &PackFile,
        archive: Option<&Path>,
        staging: &Path,
    ) -> std::result::Result<(String, StagedFile), StageError> {
        let dest = staging.join(&file.path);
        match &file.source {
            FileSource::Remote { urls } => {
                let object = match self
                    .sources
                    .cache
                    .ensure(&self.sources.downloader, urls, &file.hashes, file.size)
                    .await
                {
                    Ok(object) => object,
                    Err(e) if e.code == ErrorCode::DownloadRejected && file.manual.is_some() => {
                        let manual = file.manual.as_ref().expect("checked above");
                        return Err(StageError::Blocked(ManualFile {
                            path: file.path.clone(),
                            name: file.name.clone(),
                            file_name: manual.file_name.clone(),
                            page_url: manual.page_url.clone(),
                            hashes: file.hashes.clone(),
                        }));
                    }
                    Err(e) => return Err(e.into()),
                };
                if let Some(parent) = dest.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(LauncherError::from)?;
                }
                tokio::fs::copy(&object, &dest).await.map_err(LauncherError::from)?;
            }
            FileSource::Archive { entry } => {
                let archive = archive.ok_or_else(archive_gone)?.to_path_buf();
                let (entry, to) = (entry.clone(), dest.clone());
                let sha1 = file.hashes.sha1.clone().unwrap_or_default();
                tokio::task::spawn_blocking(move || mrpack::extract_entry(&archive, &entry, &to, &sha1))
                    .await
                    .map_err(join_error)??;
            }
        }
        let sha1 = match &file.hashes.sha1 {
            Some(s) => s.to_ascii_lowercase(),
            None => {
                let d = dest.clone();
                tokio::task::spawn_blocking(move || sha1_file(&d))
                    .await
                    .map_err(join_error)?
                    .map_err(LauncherError::from)?
            }
        };
        let size = tokio::fs::metadata(&dest).await.map_err(LauncherError::from)?.len();
        Ok((file.path.clone(), StagedFile { sha1, size }))
    }
}
```

(`?` on a `LauncherError` inside `stage_one` converts through `From<LauncherError> for StageError`.)

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --test sync`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/sync crates/launcher-core/tests/sync.rs
git commit -m "feat(core): Syncer stages, installs a changed loader first, swaps, and writes the lockfile last

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 16: CurseForge files that must be downloaded by hand

**Files:**
- Create: `crates/launcher-core/src/sync/manual.rs`
- Modify: `crates/launcher-core/src/sync/mod.rs` (`pub mod manual;`, `Syncer::import_manual`)
- Modify: `Cargo.toml` (workspace) and `crates/launcher-core/Cargo.toml`: add `dirs = "7"`
- Test: `crates/launcher-core/tests/sync.rs` (append), plus unit tests in `manual.rs`

**Interfaces:**
- Consumes: `ManualFile`, `SyncOutcome::NeedsManual` (Task 15), `ObjectCache::insert_file` (Task 6).
- Produces:
  - `launcher_core::sync::manual::downloads_dir() -> Option<PathBuf>`: the OS Downloads folder.
  - `manual::scan(dir: &Path, wanted: &[ManualFile]) -> Result<Vec<(ManualFile, PathBuf)>>` (async). It looks only at `dir`'s own `.jar`/`.zip` files of at most 512 MiB, tries the expected name first, and matches by hash, because browsers rename duplicates. A missing `dir` gives an empty list.
  - `Syncer::import_manual(&self, dir: &Path, files: &[ManualFile]) -> Result<usize>`: copies matches into the object cache (the player's file stays put); the next sync uses them.

Plan 3b's dialog polls `scan` on the Downloads folder while it is open. The CLI calls `import_manual` once (Task 19).

- [ ] **Step 1: Write the failing tests**

The test module in `manual.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::{
        hash::{curseforge_murmur2, sha1_hex},
        Hashes,
    };

    fn wanted(file_name: &str, hashes: Hashes) -> ManualFile {
        ManualFile {
            path: format!("mods/{file_name}"),
            name: file_name.into(),
            file_name: file_name.into(),
            page_url: "https://www.curseforge.com/projects/1".into(),
            hashes,
        }
    }

    #[tokio::test]
    async fn finds_renamed_downloads_by_hash() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cozy (1).jar"), b"cozy").unwrap();
        std::fs::write(dir.path().join("other.zip"), b"other").unwrap();
        std::fs::write(dir.path().join("cozy.txt"), b"cozy").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/paint.jar"), b"paint").unwrap();
        let cozy = wanted("cozy.jar", Hashes::sha1(sha1_hex(b"cozy")));
        let other = wanted("o.jar", Hashes { murmur2: Some(curseforge_murmur2(b"other")), ..Hashes::default() });
        let paint = wanted("paint.jar", Hashes::sha1(sha1_hex(b"paint")));
        let empty = wanted("nothing.jar", Hashes::default());
        let found = scan(dir.path(), &[cozy.clone(), other.clone(), paint, empty]).await.unwrap();
        assert_eq!(
            found,
            vec![(cozy, dir.path().join("cozy (1).jar")), (other, dir.path().join("other.zip"))]
        );
        assert!(scan(&dir.path().join("missing"), &[]).await.unwrap().is_empty());
    }
}
```

Append to `crates/launcher-core/tests/sync.rs`:

```rust
#[tokio::test]
async fn blocked_curseforge_files_wait_for_a_manual_download() {
    let env = Env::new().await;
    let url = PackBuilder::new(&env.server, "Tiny", "1.0.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v1", "both", None)
        .cf_mod("cozy", "cozy.jar", 1234567, 42, b"cozy", true)
        .mount(&env.server)
        .await;
    let (instance, desired) = env.instance(&url).await;
    let files = match env.sync(&instance.id, &desired).await.unwrap() {
        SyncOutcome::NeedsManual(files) => files,
        other => panic!("{other:?}"),
    };
    assert_eq!(files.len(), 1);
    assert_eq!((files[0].path.as_str(), files[0].file_name.as_str()), ("mods/cozy.jar", "cozy.jar"));
    assert_eq!(files[0].page_url, "https://www.curseforge.com/projects/42");
    assert!(!env.game_file(&instance.id, "mods/appleskin.jar").exists(), "nothing applied");
    assert!(Managed::load(&env.store.perland_dir(&instance.id)).await.is_none());

    let downloads = env._tmp.path().join("Downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    std::fs::write(downloads.join("cozy (1).jar"), b"cozy").unwrap();
    assert_eq!(env.syncer.import_manual(&downloads, &files).await.unwrap(), 1);
    done(env.sync(&instance.id, &desired).await.unwrap());
    assert_eq!(std::fs::read(env.game_file(&instance.id, "mods/cozy.jar")).unwrap(), b"cozy");
    assert!(downloads.join("cozy (1).jar").exists(), "the player's download is left alone");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib sync::manual` and `cargo test -p launcher-core --test sync blocked`
Expected: compile errors (no module `manual`, no method `import_manual`).

- [ ] **Step 3: Implement**

Add `dirs = "7"` (workspace) and `dirs.workspace = true` (`launcher-core`). Then `crates/launcher-core/src/sync/manual.rs`:

```rust
//! Files a pack's author blocks other launchers from downloading (CurseForge, spec §3.4). The
//! player downloads them in the browser, and the launcher finds them by hash.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use super::ManualFile;
use crate::{download::hash::sha1_file, error::Result};

/// The largest file looked at.
pub const MAX_CANDIDATE: u64 = 512 << 20;

/// The player's Downloads folder, if the OS has one.
pub fn downloads_dir() -> Option<PathBuf> {
    dirs::download_dir()
}

/// Files directly in `dir` that are one of `wanted`, matched by hash. A file with the expected
/// name is tried first, then any other `.jar` or `.zip`, because browsers rename duplicates
/// (`mod (1).jar`).
pub async fn scan(dir: &Path, wanted: &[ManualFile]) -> Result<Vec<(ManualFile, PathBuf)>> {
    let (dir, wanted) = (dir.to_path_buf(), wanted.to_vec());
    tokio::task::spawn_blocking(move || scan_blocking(&dir, &wanted))
        .await
        .map_err(std::io::Error::other)?
}

fn scan_blocking(dir: &Path, wanted: &[ManualFile]) -> Result<Vec<(ManualFile, PathBuf)>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || meta.len() > MAX_CANDIDATE {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".jar") || lower.ends_with(".zip") {
            candidates.push((name, entry.path()));
        }
    }
    candidates.sort();
    let mut sha1s: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut found = Vec::new();
    for file in wanted {
        if file.hashes.is_empty() {
            continue; // nothing to prove a match with
        }
        let mut ordered: Vec<&(String, PathBuf)> = candidates.iter().collect();
        ordered.sort_by_key(|(name, _)| *name != file.file_name);
        for (_, path) in ordered {
            let matches = match &file.hashes.sha1 {
                Some(expected) => sha1s
                    .entry(path.clone())
                    .or_insert_with(|| sha1_file(path).ok())
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(expected)),
                None => file.hashes.verify_file(path).unwrap_or(false),
            };
            if matches {
                found.push((file.clone(), path.clone()));
                break;
            }
        }
    }
    Ok(found)
}
```

In `sync/mod.rs`: add `pub mod manual;` and, inside `impl Syncer`:

```rust
    /// Copy every blocked file found in `dir` into the object cache (the player's copy stays
    /// where it is). Returns how many were found; the next sync uses them.
    pub async fn import_manual(&self, dir: &Path, files: &[ManualFile]) -> Result<usize> {
        let found = manual::scan(dir, files).await?;
        for (file, path) in &found {
            self.sources.cache.insert_file(path, &file.hashes).await?;
        }
        Ok(found.len())
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --lib sync::manual && cargo test -p launcher-core --test sync`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/launcher-core/Cargo.toml crates/launcher-core/src/sync crates/launcher-core/tests/sync.rs
git commit -m "feat(core): find hand-downloaded CurseForge files by hash and use them in the next sync

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 17: Adding packs, update policies, preview, and pack art

**Files:**
- Create: `crates/launcher-core/src/sources/art.rs`
- Modify: `crates/launcher-core/src/sources/mod.rs` (`pub mod art;`)
- Modify: `crates/launcher-core/src/sync/mod.rs` (`Ensured`, `add_pack`, `installed`, `check`, `preview`, `ensure`)
- Modify: `crates/launcher-core/tests/common/mod.rs` (`PackBuilder::host`)
- Test: `crates/launcher-core/tests/sync.rs` (append); unit tests in `art.rs`

**Interfaces:**
- Consumes: `Sources::{open, resolve, check}` (Task 11), `sync_locked`/`InstanceLock` (Task 15), `plan::{wanted, changes}`, `lockfile::load_pack`.
- Produces `launcher_core::sources::art`:
  - `MAX_ART_BYTES = 5 MiB`
  - `sniff(&[u8]) -> Option<&'static str>`: `png`, `jpg`, `webp` or `svg`.
  - `fetch(http, &PackArt, dir: &Path)` (async, never fails): writes `icon.<ext>` and `banner.<ext>` and removes stale other-extension copies.
  - `fetch_limited(http, &PackArt, dir, limit: usize)` (async)
- Produces, in `launcher_core::sync`:
  - `struct Ensured { outcome: SyncOutcome, warning: Option<LauncherError> }`
  - On `Syncer`:
    - `add_pack(&self, store, input: &PackInput) -> Result<(Instance, DesiredState)>`:
      - The name is the pack's (or `"Pack"`), and the game is the pack's.
      - The source is set.
      - The policy is `about.policy`, else `Ask`.
      - The art is fetched into `.perland/art/`.
      - Its files arrive with the first sync.
    - `installed(&self, store, id) -> Option<DesiredState>`: `pack.json`.
    - `check(&self, store, instance: &Instance) -> UpdateCheck`: `UpToDate` when never synced.
    - `preview(&self, store, instance: &Instance, desired: &DesiredState) -> Changes`
    - `ensure(&self, store, id, sink) -> Result<Ensured>`: the Play-time sync.

**`ensure` follows the policy** (spec §3.1):
1. Take the instance lock.
2. Pick what to sync to:
   - **No `pack.json` yet:** resolve the source. Errors propagate.
   - **`pinned`:** the installed snapshot.
   - **Otherwise, check for an update:**
     - `UpToDate`: the snapshot.
     - `Unavailable(e)`: the snapshot, with `warning = e`.
     - `Available { source, .. }`: resolve `source`. If that fails, use the snapshot with the error as the warning.
3. Sync, with the lock held.
4. If a newer source (Modrinth) was synced (`Done`), save it on the instance.
5. Refresh the art after a sync that changed something.

In 3a, `ask` behaves like `auto` here. In Plan 3b the app asks first (`check` + `preview`), then calls `sync`.

- [ ] **Step 1: Write the failing tests**

The test module in `crates/launcher-core/src/sources/art.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::{method, path}, Mock, MockServer, ResponseTemplate};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn sniffs_only_images() {
        assert_eq!(sniff(PNG), Some("png"));
        assert_eq!(sniff(b"\xff\xd8\xff\xe0rest"), Some("jpg"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(sniff(b"<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>"), Some("svg"));
        assert_eq!(sniff(b"<!doctype html><html>"), None);
        assert_eq!(sniff(b""), None);
    }

    async fn serve(server: &MockServer, at: &str, body: &[u8]) {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn fetches_images_and_ignores_everything_else() {
        let server = MockServer::start().await;
        serve(&server, "/icon.png", PNG).await;
        serve(&server, "/banner", &b"<html>login page</html>"[..]).await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("icon.svg"), "<svg/>").unwrap();
        let art = PackArt {
            icon: Some(format!("{}/icon.png", server.uri())),
            banner: Some(format!("{}/banner", server.uri())),
        };
        fetch(&reqwest::Client::new(), &art, dir.path()).await;
        assert_eq!(std::fs::read(dir.path().join("icon.png")).unwrap(), PNG);
        assert!(!dir.path().join("icon.svg").exists(), "the old icon is replaced");
        assert!(std::fs::read_dir(dir.path()).unwrap().all(|e| {
            !e.unwrap().file_name().to_string_lossy().starts_with("banner")
        }));

        let big = MockServer::start().await;
        serve(&big, "/icon.png", &[PNG, &[0u8; 64]].concat()).await;
        let small = tempfile::tempdir().unwrap();
        let art = PackArt { icon: Some(format!("{}/icon.png", big.uri())), banner: None };
        fetch_limited(&reqwest::Client::new(), &art, small.path(), 32).await;
        assert!(!small.path().join("icon.png").exists(), "over the limit");
    }
}
```

In `tests/common/mod.rs`, add to `PackBuilder`:

```rust
    /// Any extra file at `at` (an absolute path on the server, like `/pack/icon.png`).
    pub fn host(mut self, at: &str, body: &[u8]) -> Self {
        self.hosted.push((at.into(), body.to_vec()));
        self
    }
```

Append to `crates/launcher-core/tests/sync.rs`. Add `instances::{SourceRef, UpdatePolicy}` and `sources::UpdateCheck` to the imports.

```rust
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

#[tokio::test]
async fn add_pack_names_the_instance_after_the_pack_and_takes_its_policy() {
    let env = Env::new().await;
    let url = v1(&env.server)
        .sidecar("[art]\nicon = \"icon.png\"\n[sync]\npolicy = \"auto\"\n")
        .host("/pack/icon.png", PNG)
        .mount(&env.server)
        .await;
    let (instance, desired) = env
        .syncer
        .add_pack(&env.store, &PackInput::Packwiz { url: url.clone() })
        .await
        .unwrap();
    assert_eq!(instance.name, "Tiny");
    assert_eq!(instance.source, SourceRef::Packwiz { url });
    assert_eq!(instance.update_policy, UpdatePolicy::Auto);
    assert_eq!(instance.game.loader.clone().unwrap().version, "47.4.22");
    assert_eq!(env.store.load(&instance.id).await.unwrap(), instance);
    let art = env.store.perland_dir(&instance.id).join("art/icon.png");
    assert_eq!(std::fs::read(art).unwrap(), PNG);
    assert!(!env.game_file(&instance.id, "mods/appleskin.jar").exists(), "files come with the first sync");
    done(env.sync(&instance.id, &desired).await.unwrap());
    assert!(env.game_file(&instance.id, "mods/appleskin.jar").exists());
}

#[tokio::test]
async fn ensure_takes_updates_and_preview_names_them() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, _) = env.syncer.add_pack(&env.store, &PackInput::Packwiz { url }).await.unwrap();
    assert_eq!(instance.update_policy, UpdatePolicy::Ask, "no sidecar: ask");
    let first = env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();
    assert!(first.warning.is_none());
    assert!(!done(first.outcome).up_to_date);
    assert_eq!(env.syncer.check(&env.store, &instance).await, UpdateCheck::UpToDate);

    env.server.reset().await;
    PackBuilder::new(&env.server, "Tiny", "1.1.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v1", "both", None)
        .url_mod("jei", "jei.jar", b"jei", "both", None)
        .file("config/a.toml", b"a = 1", true)
        .mount(&env.server)
        .await;
    let UpdateCheck::Available { source, version, .. } = env.syncer.check(&env.store, &instance).await else {
        panic!("expected an update");
    };
    assert_eq!(version, "1.1.0");
    let v2 = env.syncer.sources.resolve(&source).await.unwrap();
    let preview = env.syncer.preview(&env.store, &instance, &v2).await;
    assert_eq!((preview.added.clone(), preview.removed.clone()), (vec!["jei".to_string()], vec!["zoom".to_string()]));
    assert_eq!(preview.from_version.as_deref(), Some("1.0.0"));

    let report = done(env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap().outcome);
    assert_eq!(report.changes, preview);
    assert!(env.game_file(&instance.id, "mods/jei.jar").exists());
    assert!(!env.game_file(&instance.id, "mods/zoom.jar").exists());
}

#[tokio::test]
async fn pinned_instances_stay_on_their_version() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (mut instance, _) = env.syncer.add_pack(&env.store, &PackInput::Packwiz { url }).await.unwrap();
    env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();
    instance.update_policy = UpdatePolicy::Pinned;
    env.store.save(&instance).await.unwrap();

    env.server.reset().await;
    v1(&env.server).with_version("9.9.9").url_mod("jei", "jei.jar", b"jei", "both", None).mount(&env.server).await;
    let ensured = env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();
    assert!(done(ensured.outcome).up_to_date);
    assert!(!env.game_file(&instance.id, "mods/jei.jar").exists());
    assert_eq!(hits(&env.server, "/pack/pack.toml").await, 0, "pinned never checks");
}

#[tokio::test]
async fn offline_ensure_uses_the_installed_snapshot_with_a_warning() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, _) = env.syncer.add_pack(&env.store, &PackInput::Packwiz { url }).await.unwrap();
    env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();
    std::fs::remove_file(env.game_file(&instance.id, "mods/zoom.jar")).unwrap();

    env.server.reset().await; // the pack host is gone: everything answers 404
    let ensured = env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();
    assert_eq!(ensured.warning.unwrap().code, ErrorCode::UpdateCheckFailed);
    done(ensured.outcome);
    assert!(env.game_file(&instance.id, "mods/zoom.jar").exists(), "repaired from the cache");

    // A pack that was never synced can't be played offline: a clear error, nothing half-made.
    let mut never = env.store.create("Never", instance.game.clone()).await.unwrap();
    never.source = SourceRef::Packwiz { url: format!("{}/other/pack.toml", env.server.uri()) };
    env.store.save(&never).await.unwrap();
    let err = env.syncer.ensure(&env.store, &never.id, &env.sink()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PackNotFound);
    assert!(Managed::load(&env.store.perland_dir(&never.id)).await.is_none());

    let vanilla = env.store.create("Vanilla", instance.game.clone()).await.unwrap();
    assert_eq!(
        env.syncer.ensure(&env.store, &vanilla.id, &env.sink()).await.unwrap_err().code,
        ErrorCode::NoPack
    );
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib sources::art` and `cargo test -p launcher-core --test sync`
Expected: compile errors (no module `art`; no methods `add_pack`, `ensure`, `check`, `preview`).

- [ ] **Step 3: Implement**

`crates/launcher-core/src/sources/art.rs` (and `pub mod art;`):

```rust
//! Pack art for the app. It's downloaded into the instance because the app's CSP allows no
//! remote images. Only real images under a size cap are kept, and anything else leaves the
//! generated art in place.
use std::path::Path;

use super::{check_pack_url, PackArt};
use crate::fsutil::write_atomic;

pub const MAX_ART_BYTES: usize = 5 << 20;
const EXTENSIONS: [&str; 4] = ["png", "jpg", "webp", "svg"];

/// The image type, by signature. SVG has none, so it must start like XML or `<svg`.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
    let start = head.trim_start_matches('\u{feff}').trim_start();
    ((start.starts_with("<svg") || start.starts_with("<?xml")) && head.contains("<svg")).then_some("svg")
}

pub async fn fetch(http: &reqwest::Client, art: &PackArt, dir: &Path) {
    fetch_limited(http, art, dir, MAX_ART_BYTES).await
}

/// Save `art` as `icon.<ext>` and `banner.<ext>` in `dir`. Never fails.
pub async fn fetch_limited(http: &reqwest::Client, art: &PackArt, dir: &Path, limit: usize) {
    for (stem, url) in [("icon", &art.icon), ("banner", &art.banner)] {
        let Some(url) = url else { continue };
        let Some((bytes, ext)) = download(http, url, limit).await else { continue };
        if write_atomic(&dir.join(format!("{stem}.{ext}")), &bytes).await.is_err() {
            continue;
        }
        for other in EXTENSIONS.iter().filter(|e| **e != ext) {
            let _ = tokio::fs::remove_file(dir.join(format!("{stem}.{other}"))).await;
        }
    }
}

async fn download(http: &reqwest::Client, url: &str, limit: usize) -> Option<(Vec<u8>, &'static str)> {
    check_pack_url(url).ok()?;
    let mut response = http.get(url).send().await.ok()?;
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > limit as u64) {
        return None;
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        body.extend_from_slice(&chunk);
        if body.len() > limit {
            return None;
        }
    }
    let ext = sniff(&body)?;
    Some((body, ext))
}
```

In `sync/mod.rs`, add the imports `crate::instances::{Instance, SourceRef, UpdatePolicy}`, `crate::sources::{art, no_pack, PackInput, UpdateCheck}` and `lockfile::load_pack`, then:

```rust
/// What `ensure` did, and whether the update check failed along the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ensured {
    pub outcome: SyncOutcome,
    /// The update check (or resolving the update) failed, so the installed version was used.
    pub warning: Option<LauncherError>,
}
```

And inside `impl Syncer`:

```rust
    /// A new instance for a pack, named after it, with its game version and its preferred update
    /// policy (else "ask"). Its files arrive with the first sync of the returned pack.
    pub async fn add_pack(&self, store: &InstanceStore, input: &PackInput) -> Result<(Instance, DesiredState)> {
        let (source, desired) = self.sources.open(input).await?;
        let name = if desired.name.trim().is_empty() { "Pack" } else { desired.name.as_str() };
        let mut instance = store
            .create(name, GameSpec { minecraft: desired.minecraft.clone(), loader: desired.loader.clone() })
            .await?;
        instance.source = source;
        instance.update_policy = desired.about.policy.unwrap_or(UpdatePolicy::Ask);
        store.save(&instance).await?;
        art::fetch(&self.sources.http, &desired.about.art, &store.perland_dir(&instance.id).join("art")).await;
        Ok((instance, desired))
    }

    /// The pack version installed in `id` (`pack.json`), if it was ever synced.
    pub async fn installed(&self, store: &InstanceStore, id: &str) -> Option<DesiredState> {
        load_pack(&store.perland_dir(id)).await
    }

    /// Is there an update? `UpToDate` for an instance that was never synced: its first sync
    /// installs the newest version anyway.
    pub async fn check(&self, store: &InstanceStore, instance: &Instance) -> UpdateCheck {
        match self.installed(store, &instance.id).await {
            Some(installed) => self.sources.check(&instance.source, &installed).await,
            None => UpdateCheck::UpToDate,
        }
    }

    /// What syncing to `desired` would change: for the app's "Update ready" dialog.
    pub async fn preview(&self, store: &InstanceStore, instance: &Instance, desired: &DesiredState) -> Changes {
        let managed = Managed::load(&store.perland_dir(&instance.id)).await;
        let wanted = plan::wanted(desired, &instance.optional_choices);
        plan::changes(&wanted, managed.as_ref(), &desired.pack_version)
    }

    /// Bring a pack instance up to date before playing, following its update policy (core spec
    /// §3.1). Pinned stays on the installed version. Auto and ask take updates (the app asks
    /// first, with `check` and `preview`). A failed check plays what's installed, with a warning.
    pub async fn ensure(&self, store: &InstanceStore, id: &str, sink: &EventSink) -> Result<Ensured> {
        let lock = InstanceLock::try_acquire(&store.perland_dir(id))?;
        let instance = store.load(id).await?;
        if instance.source == SourceRef::None {
            return Err(no_pack());
        }
        let mut warning = None;
        let mut new_source = None;
        let desired = match self.installed(store, id).await {
            None => self.sources.resolve(&instance.source).await?,
            Some(installed) if instance.update_policy == UpdatePolicy::Pinned => installed,
            Some(installed) => match self.sources.check(&instance.source, &installed).await {
                UpdateCheck::UpToDate => installed,
                UpdateCheck::Unavailable(e) => {
                    warning = Some(e);
                    installed
                }
                UpdateCheck::Available { source, .. } => match self.sources.resolve(&source).await {
                    Ok(update) => {
                        new_source = (source != instance.source).then_some(source);
                        update
                    }
                    Err(e) => {
                        warning = Some(e);
                        installed
                    }
                },
            },
        };
        let outcome = self.sync_locked(&lock, store, id, &desired, sink).await?;
        if let SyncOutcome::Done(report) = &outcome {
            if let Some(source) = new_source {
                // Only now: a source that names a version whose sync never finished would hide
                // the update from the next check.
                let mut fresh = store.load(id).await?;
                fresh.source = source;
                store.save(&fresh).await?;
            }
            if !report.up_to_date {
                art::fetch(&self.sources.http, &desired.about.art, &store.perland_dir(id).join("art")).await;
            }
        }
        Ok(Ensured { outcome, warning })
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src crates/launcher-core/tests
git commit -m "feat(core): add packs as instances, sync by update policy, preview updates, fetch pack art

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 18: End-to-end sync scenarios

These are spec §7's scenarios against the tiny wiremock pack: update, removal, "Your additions", `preserve`, an interrupted sync followed by a resume, optional toggles, and the `.mrpack` override rule. Production code should not need changes. If a test fails, fix the bug in the owning module and say so in the commit.

**Files:**
- Test: `crates/launcher-core/tests/sync.rs` (append)

**Interfaces:**
- Consumes: everything from Tasks 15–17. Add these imports: `std::time::Duration`, `launcher_core::sync::lockfile::additions`, `wiremock::{Mock, ResponseTemplate, matchers::{method, path}}`, `common::write_zip`, `serde_json::json`.

- [ ] **Step 1: Write the tests**

```rust
fn trashed(env: &Env, id: &str, rel: &str) -> Option<PathBuf> {
    let trash = env.store.perland_dir(id).join("trash");
    std::fs::read_dir(trash).ok()?.flatten().map(|d| d.path().join(rel)).find(|p| p.exists())
}

#[tokio::test]
async fn update_keeps_additions_and_preserved_configs() {
    let env = Env::new().await;
    let url = v1(&env.server)
        .url_mod("gone", "gone.jar", b"gone", "both", None)
        .mount(&env.server)
        .await;
    let (instance, _) = env.syncer.add_pack(&env.store, &PackInput::Packwiz { url }).await.unwrap();
    env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();
    std::fs::write(env.game_file(&instance.id, "mods/mine.jar"), b"my own mod").unwrap();
    std::fs::write(env.game_file(&instance.id, "config/a.toml"), b"a = 42 # mine").unwrap();

    env.server.reset().await;
    PackBuilder::new(&env.server, "Tiny", "2.0.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v2", "both", None)
        .url_mod("zoom", "zoom.jar", b"zoom", "client", None)
        .url_mod("jei", "jei.jar", b"jei", "both", None)
        .file("config/a.toml", b"a = 2", true)
        .mount(&env.server)
        .await;
    let report = done(env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap().outcome);

    assert_eq!(report.changes.added, vec!["jei"]);
    assert_eq!(report.changes.updated, vec!["appleskin"]);
    assert_eq!(report.changes.removed, vec!["gone"]);
    assert_eq!(std::fs::read(env.game_file(&instance.id, "mods/appleskin.jar")).unwrap(), b"apple v2");
    assert_eq!(std::fs::read(env.game_file(&instance.id, "mods/mine.jar")).unwrap(), b"my own mod");
    assert_eq!(std::fs::read(env.game_file(&instance.id, "config/a.toml")).unwrap(), b"a = 42 # mine");
    assert!(!env.game_file(&instance.id, "mods/gone.jar").exists());
    assert!(trashed(&env, &instance.id, "mods/gone.jar").is_some(), "removed files go to the trash");
    let managed = Managed::load(&env.store.perland_dir(&instance.id)).await;
    assert_eq!(
        additions(&env.store.game_dir(&instance.id), managed.as_ref()).await.unwrap(),
        vec!["mine.jar"]
    );
}

#[tokio::test]
async fn a_player_file_where_the_pack_now_puts_one_goes_to_the_trash() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, _) = env.syncer.add_pack(&env.store, &PackInput::Packwiz { url }).await.unwrap();
    env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();
    std::fs::write(env.game_file(&instance.id, "mods/jei.jar"), b"player's jei").unwrap();

    env.server.reset().await;
    v1(&env.server).with_version("1.1.0").url_mod("jei", "jei.jar", b"pack jei", "both", None).mount(&env.server).await;
    env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();
    assert_eq!(std::fs::read(env.game_file(&instance.id, "mods/jei.jar")).unwrap(), b"pack jei");
    let kept = trashed(&env, &instance.id, "mods/jei.jar").expect("kept in the trash");
    assert_eq!(std::fs::read(kept).unwrap(), b"player's jei");
}

#[tokio::test]
async fn optional_toggles_apply_from_the_snapshot() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (mut instance, _) = env.syncer.add_pack(&env.store, &PackInput::Packwiz { url }).await.unwrap();
    instance.update_policy = UpdatePolicy::Pinned; // no update checks: only the snapshot
    env.store.save(&instance).await.unwrap();
    env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();
    assert!(!env.game_file(&instance.id, "mods/shaders.jar").exists());
    let pack_reads = hits(&env.server, "/pack/pack.toml").await;

    instance.optional_choices.insert("mods/shaders.jar".into(), true);
    env.store.save(&instance).await.unwrap();
    let report = done(env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap().outcome);
    assert!(!report.up_to_date);
    assert!(env.game_file(&instance.id, "mods/shaders.jar").exists());

    instance.optional_choices.insert("mods/shaders.jar".into(), false);
    env.store.save(&instance).await.unwrap();
    env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();
    assert!(!env.game_file(&instance.id, "mods/shaders.jar").exists());
    assert!(trashed(&env, &instance.id, "mods/shaders.jar").is_some());
    assert_eq!(
        hits(&env.server, "/pack/pack.toml").await,
        pack_reads,
        "toggles apply from the installed snapshot, without reading the pack"
    );
}

#[tokio::test]
async fn interrupted_sync_keeps_the_old_state_and_resumes_from_the_cache() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.syncer.add_pack(&env.store, &PackInput::Packwiz { url }).await.unwrap();
    env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap();

    env.server.reset().await;
    v1(&env.server)
        .with_version("1.1.0")
        .url_mod("alpha", "alpha.jar", b"alpha", "both", None)
        .url_mod("beta", "beta.jar", b"beta", "both", None)
        .mount(&env.server)
        .await;
    // beta.jar fails (slowly, so alpha finishes first) for the whole first attempt.
    Mock::given(method("GET"))
        .and(path("/files/beta.jar"))
        .respond_with(ResponseTemplate::new(500).set_delay(Duration::from_millis(100)))
        .with_priority(1)
        .up_to_n_times(4)
        .mount(&env.server)
        .await;
    let err = env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::HostTrouble);
    let managed = Managed::load(&env.store.perland_dir(&instance.id)).await.unwrap();
    assert_eq!(managed.stamp, desired.stamp, "the old lockfile still describes the instance");
    assert!(!env.game_file(&instance.id, "mods/alpha.jar").exists(), "nothing was swapped in");
    assert!(env.game_file(&instance.id, "mods/zoom.jar").exists());

    done(env.syncer.ensure(&env.store, &instance.id, &env.sink()).await.unwrap().outcome);
    assert!(env.game_file(&instance.id, "mods/alpha.jar").exists());
    assert!(env.game_file(&instance.id, "mods/beta.jar").exists());
    assert_eq!(hits(&env.server, "/files/alpha.jar").await, 1, "alpha came from the cache the second time");
}

fn mrpack(env: &Env, name: &str, overrides: &[(&str, &[u8])]) -> PathBuf {
    let index = serde_json::to_vec(&json!({
        "formatVersion": 1, "game": "minecraft", "versionId": name, "name": "Sky",
        "files": [], "dependencies": { "minecraft": "1.20.1" }
    }))
    .unwrap();
    let path = env._tmp.path().join(format!("{name}.mrpack"));
    let mut entries: Vec<(&str, &[u8])> = vec![("modrinth.index.json", &index[..])];
    entries.extend_from_slice(overrides);
    write_zip(&path, &entries);
    path
}

#[tokio::test]
async fn mrpack_override_edits_survive_an_update() {
    let env = Env::new().await;
    let v1 = mrpack(&env, "1.0", &[("overrides/config/c.toml", &b"v1"[..]), ("overrides/options.txt", &b"fov:1"[..])]);
    let (instance, desired) = env.syncer.add_pack(&env.store, &PackInput::MrpackFile { path: v1 }).await.unwrap();
    assert_eq!(instance.game.loader, None);
    done(env.sync(&instance.id, &desired).await.unwrap());
    std::fs::write(env.game_file(&instance.id, "config/c.toml"), b"mine").unwrap();

    let v2 = mrpack(&env, "2.0", &[("overrides/config/c.toml", &b"v2"[..]), ("overrides/options.txt", &b"fov:2"[..])]);
    let (_, desired2) = env.syncer.sources.open(&PackInput::MrpackFile { path: v2 }).await.unwrap();
    let report = done(env.sync(&instance.id, &desired2).await.unwrap());
    assert_eq!(report.kept_modified, vec!["config/c.toml"]);
    assert_eq!(std::fs::read(env.game_file(&instance.id, "config/c.toml")).unwrap(), b"mine");
    assert_eq!(std::fs::read(env.game_file(&instance.id, "options.txt")).unwrap(), b"fov:2");
}
```

- [ ] **Step 2: Run them**

Run: `cargo test -p launcher-core --test sync`
Expected: PASS. If one fails, it has found a bug:
1. Use superpowers:systematic-debugging.
2. Fix the bug in the owning module, with a unit test there if the gap is local.
3. Never weaken the scenario.

- [ ] **Step 3: Commit**

```bash
git add crates/launcher-core/tests/sync.rs
git commit -m "test(core): end-to-end sync: updates, additions, preserve, trash, resume, optionals, overrides

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 19: CLI: `pack add`, `pack check`, `pack additions`, `sync`, and `launch` syncs first

`perland-cli` is a dev harness with no test suite of its own. Its behavior is covered by the core tests, the nightly real-pack job (Task 20) and the manual verification (Task 21).

**Files:**
- Modify: `crates/perland-cli/src/main.rs`
- Modify: `crates/perland-cli/Cargo.toml` (add `reqwest.workspace = true` if the shared HTTP client type is needed there)

**Interfaces:**
- Consumes: `Syncer::{for_installer, add_pack, sync, ensure, check, import_manual}`, `SyncOutcome`, `SyncReport`, `manual::downloads_dir`, `lockfile::{Managed, additions}`, `PackInput::parse`, `InstanceLock`.
- Produces these commands. Machine-readable output goes to stdout; progress and warnings go to stderr.
  - `perland-cli pack add <input>`: creates the instance and syncs it, then prints the instance id on stdout. `<input>` is a `pack.toml` link, a `.mrpack` path, or `modrinth:<slug>[@version]`.
  - `perland-cli pack check --instance <id>`: prints `up to date`, `update available: <version>`, or `couldn't check: <message>`.
  - `perland-cli pack additions --instance <id>`: one file name per line.
  - `perland-cli sync --instance <id> [--manual-dir <dir>]`: prints `up to date` or the changelog line. It exits non-zero with `manual_downloads_needed` if blocked files remain after looking in `--manual-dir` (default: the Downloads folder).
  - `perland-cli launch --instance <id> [--dry-run] [--no-sync]`: syncs pack instances first unless `--no-sync`, then holds the instance lock until the game exits.

- [ ] **Step 1: Implement**

Add to `Cmd`:

```rust
    /// Packs: add one as a new instance, check for updates, list the player's own mods
    Pack {
        #[command(subcommand)]
        cmd: PackCmd,
    },
    /// Bring a pack instance up to date (downloads only what changed)
    Sync {
        #[arg(long)]
        instance: String,
        /// Where to look for files that must be downloaded by hand (default: your Downloads folder)
        #[arg(long)]
        manual_dir: Option<PathBuf>,
    },
```

Add `#[arg(long)] no_sync: bool` (doc: `/// Don't update the pack first`) to `Launch`, and:

```rust
#[derive(Subcommand)]
enum PackCmd {
    /// Create an instance from a pack link, a .mrpack file, or modrinth:<slug>[@version]; prints its id
    Add { input: String },
    /// Say whether an update is available
    Check {
        #[arg(long)]
        instance: String,
    },
    /// List the jars in mods/ that the pack doesn't manage
    Additions {
        #[arg(long)]
        instance: String,
    },
}
```

Shared helpers (replace the inline `Installer` construction in `launch_instance` with `installer(dirs)`):

```rust
fn installer(dirs: &LauncherDirs) -> Result<(Arc<Installer>, reqwest::Client), LauncherError> {
    let http = launcher_core::http::client(env!("CARGO_PKG_VERSION"))?;
    let installer = Installer {
        dirs: dirs.clone(),
        meta: MetaClient::new(http.clone(), dirs.meta()),
        downloader: Downloader::new(http.clone()),
        platform: Platform::current(),
    };
    Ok((Arc::new(installer), http))
}

fn syncer(dirs: &LauncherDirs) -> Result<Syncer, LauncherError> {
    let (installer, http) = installer(dirs)?;
    Ok(Syncer::for_installer(installer, http))
}

fn progress_sink() -> EventSink {
    Arc::new(|event| match event {
        Event::Progress { task, done, total, .. } => eprint!("\r{}: {done}/{total}    ", task.key),
        Event::Log { line } => println!("{line}"),
        Event::Warning(e) => eprintln!("\nwarning: {}", e.message),
        Event::StateChanged { .. } => {}
    })
}

/// Print what a sync did. Returns the files still to download by hand, if any.
fn report(outcome: &SyncOutcome) -> Option<&[ManualFile]> {
    eprintln!();
    match outcome {
        SyncOutcome::Done(r) if r.up_to_date => println!("up to date"),
        SyncOutcome::Done(r) => {
            let c = &r.changes;
            println!(
                "{} → {}: +{} added · {} updated · {} removed{}",
                c.from_version.as_deref().unwrap_or("new"),
                c.to_version,
                c.added.len(),
                c.updated.len(),
                c.removed.len(),
                if r.game_changed { " (new game version)" } else { "" }
            );
            for path in &r.kept_modified {
                println!("kept your version of {path}");
            }
        }
        SyncOutcome::NeedsManual(files) => {
            eprintln!("{} files must be downloaded by hand:", files.len());
            for f in files {
                eprintln!("  {}: download {} from {}", f.name, f.file_name, f.page_url);
            }
            return Some(files);
        }
    }
    None
}

/// Sync `id` by its policy, picking up hand-downloaded files from `manual_dir` once.
async fn sync_instance(
    syncer: &Syncer,
    store: &InstanceStore,
    id: &str,
    manual_dir: Option<PathBuf>,
) -> Result<(), LauncherError> {
    let sink = progress_sink();
    let ensured = syncer.ensure(store, id, &sink).await?;
    if let Some(w) = &ensured.warning {
        eprintln!("warning: {}", w.message);
    }
    let Some(files) = report(&ensured.outcome).map(<[ManualFile]>::to_vec) else {
        return Ok(());
    };
    if let Some(dir) = manual_dir.or_else(launcher_core::sync::manual::downloads_dir) {
        if syncer.import_manual(&dir, &files).await? > 0 {
            let again = syncer.ensure(store, id, &sink).await?;
            if report(&again.outcome).is_none() {
                return Ok(());
            }
        }
    }
    Err(LauncherError::pack(
        ErrorCode::ManualDownloadsNeeded,
        format!("{} files have to be downloaded by hand", files.len()),
    )
    .with_param("count", files.len())
    .with_fix("Download them from CurseForge, then run sync again"))
}
```

The new arms in `run`:

```rust
        Cmd::Pack { cmd: PackCmd::Add { input } } => {
            let syncer = syncer(&dirs)?;
            let (instance, desired) = syncer.add_pack(&store, &PackInput::parse(&input)?).await?;
            let outcome = syncer.sync(&store, &instance.id, &desired, &progress_sink()).await?;
            if report(&outcome).is_some() {
                eprintln!("then run: perland-cli sync --instance {}", instance.id);
            }
            println!("{}", instance.id);
        }
        Cmd::Pack { cmd: PackCmd::Check { instance } } => {
            let instance = store.load(&instance).await?;
            match syncer(&dirs)?.check(&store, &instance).await {
                UpdateCheck::UpToDate => println!("up to date"),
                UpdateCheck::Available { version, .. } => println!("update available: {version}"),
                UpdateCheck::Unavailable(e) => println!("couldn't check: {}", e.message),
            }
        }
        Cmd::Pack { cmd: PackCmd::Additions { instance } } => {
            store.load(&instance).await?;
            let managed = Managed::load(&store.perland_dir(&instance)).await;
            for name in additions(&store.game_dir(&instance), managed.as_ref()).await? {
                println!("{name}");
            }
        }
        Cmd::Sync { instance, manual_dir } => {
            store.load(&instance).await?;
            sync_instance(&syncer(&dirs)?, &store, &instance, manual_dir).await?
        }
        Cmd::Launch { instance, dry_run, no_sync } => {
            launch_instance(&dirs, &store, &accounts, &instance, dry_run, no_sync).await?
        }
```

In `launch_instance(…, no_sync: bool)`, first thing after loading the account:

```rust
    let mut instance = store.load(id).await?;
    if instance.source != SourceRef::None && !no_sync {
        sync_instance(&syncer(dirs)?, store, id, None).await?;
        instance = store.load(id).await?; // a pack update can change the game version
    }
    // Held until the game exits: a sync from the app or another CLI must wait for it.
    let _lock = InstanceLock::try_acquire(&store.perland_dir(id))?;
```

Then use `installer(dirs)?` for the rest of the existing body: `let (installer, _) = installer(dirs)?;` and `play::prepare(&installer, &instance, &sink)`. Update the imports for the new names:
- `launcher_core::instances::SourceRef`
- `launcher_core::locks::InstanceLock`
- `launcher_core::sources::{PackInput, UpdateCheck}`
- `launcher_core::sync::{lockfile::{additions, Managed}, ManualFile, SyncOutcome, Syncer}`

The existing `sink` closure in `launch_instance` becomes `progress_sink()`.

- [ ] **Step 2: Build and smoke it against a local folder (no network)**

Run:
```bash
cargo build -p perland-cli
cargo run -q -p perland-cli -- --help
cargo run -q -p perland-cli -- pack add "ftp://example.com/pack.toml"
```
Expected:
- `--help` lists `pack` and `sync`.
- `pack add` exits 1 with `error [Pack]: That doesn't look like a pack link`.

- [ ] **Step 3: Gate and commit**

Run the full pre-commit gate, then:

```bash
git add crates/perland-cli
git commit -m "feat(cli): pack add/check/additions, sync with manual downloads, launch syncs first

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 20: Nightly CI: the real Asalith pack, and fuzzing the pack parsers

Fuzzing runs **only** in CI (the repo rule; see `CLAUDE.md`). Nothing in this task is run locally except `cargo test --workspace`, which doesn't build the fuzz crate.

**Files:**
- Create: `fuzz/Cargo.toml`, `fuzz/.gitignore`, `fuzz/fuzz_targets/{packwiz_pack,packwiz_index,packwiz_meta,mrpack_index,sidecar,pack_paths}.rs`
- Modify: `Cargo.toml` (add `exclude = ["fuzz"]` to `[workspace]`)
- Modify: `.github/workflows/nightly.yml`

**Interfaces:**
- Consumes these public, pure parsers:
  - `sources::packwiz::{parse_pack, parse_index, parse_meta, meta_file}`
  - `sources::mrpack::parse_index`
  - `sources::sidecar::parse_sidecar`
  - `fsutil::{normalize_rel, safe_component}`

- [ ] **Step 1: Write the fuzz crate**

`fuzz/Cargo.toml`:

```toml
[package]
name = "launcher-fuzz"
version = "0.0.0"
publish = false
edition = "2021"
license = "GPL-3.0-only"

[package.metadata]
cargo-fuzz = true

[dependencies]
libfuzzer-sys = "0.4"
launcher-core = { path = "../crates/launcher-core" }

# Its own workspace: `cargo test --workspace` never builds it (it needs nightly and libFuzzer).
[workspace]
members = ["."]

[[bin]]
name = "packwiz_pack"
path = "fuzz_targets/packwiz_pack.rs"
test = false
doc = false
bench = false

[[bin]]
name = "packwiz_index"
path = "fuzz_targets/packwiz_index.rs"
test = false
doc = false
bench = false

[[bin]]
name = "packwiz_meta"
path = "fuzz_targets/packwiz_meta.rs"
test = false
doc = false
bench = false

[[bin]]
name = "mrpack_index"
path = "fuzz_targets/mrpack_index.rs"
test = false
doc = false
bench = false

[[bin]]
name = "sidecar"
path = "fuzz_targets/sidecar.rs"
test = false
doc = false
bench = false

[[bin]]
name = "pack_paths"
path = "fuzz_targets/pack_paths.rs"
test = false
doc = false
bench = false
```

`fuzz/.gitignore`:

```
target
corpus
artifacts
coverage
```

`fuzz/fuzz_targets/packwiz_pack.rs`:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = launcher_core::sources::packwiz::parse_pack(data);
});
```

`fuzz/fuzz_targets/packwiz_index.rs`:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = launcher_core::sources::packwiz::parse_index(data);
});
```

`fuzz/fuzz_targets/packwiz_meta.rs`:

```rust
#![no_main]
use launcher_core::{
    fsutil::normalize_rel,
    sources::{packwiz, SourceConfig},
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(meta) = packwiz::parse_meta(data, "mods/x.pw.toml") {
        if let Ok(file) = packwiz::meta_file(&SourceConfig::default(), "mods/x.pw.toml", &meta, false) {
            // Whatever the metafile says, the file it installs stays a canonical relative path.
            assert_eq!(normalize_rel(&file.path).as_deref(), Some(file.path.as_str()));
        }
    }
});
```

`fuzz/fuzz_targets/mrpack_index.rs`:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = launcher_core::sources::mrpack::parse_index(data);
});
```

`fuzz/fuzz_targets/sidecar.rs`:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let about = launcher_core::sources::sidecar::parse_sidecar(text, "https://example.com/p/pack.toml");
        for url in [about.art.icon, about.art.banner].into_iter().flatten() {
            assert!(url.starts_with("https://"), "{url}");
        }
    }
});
```

`fuzz/fuzz_targets/pack_paths.rs`:

```rust
#![no_main]
use launcher_core::fsutil::{normalize_rel, safe_component};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Some(n) = normalize_rel(text) {
            assert_eq!(normalize_rel(&n).as_deref(), Some(n.as_str()), "canonical form is stable");
            assert!(n.split('/').all(safe_component));
            assert!(!n.contains('\\') && !n.contains(':'));
        }
    }
});
```

In the root `Cargo.toml` `[workspace]`, add `exclude = ["fuzz"]`.

- [ ] **Step 2: Extend `.github/workflows/nightly.yml`**

Add, right after the Forge step in `real-install`:

```yaml
      - name: Sync Asalith Fields (packwiz + Forge 1.20.1), then launch dry-run
        shell: bash
        run: |
          ROOT="$RUNNER_TEMP/perland"
          ID=$(cargo run -q -p perland-cli -- --root "$ROOT" pack add https://perlanddev.github.io/asalith-modpack/pack.toml)
          test -n "$ID"
          cargo run -q -p perland-cli -- --root "$ROOT" launch --instance "$ID" --dry-run
          JARS=$(ls "$ROOT/instances/$ID/minecraft/mods/"*.jar | wc -l)
          echo "client jars installed: $JARS"
          test "$JARS" -ge 50
          test -f "$ROOT/instances/$ID/.perland/managed.json"
          test -z "$(cargo run -q -p perland-cli -- --root "$ROOT" pack additions --instance "$ID")"
          cargo run -q -p perland-cli -- --root "$ROOT" sync --instance "$ID" | grep -qx "up to date"
```

Add a second job:

```yaml
  fuzz:
    # Fuzzing never runs on a developer PC (CLAUDE.md). libFuzzer needs nightly Rust on Linux.
    runs-on: ubuntu-latest
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: fuzz
      - run: cargo install cargo-fuzz --locked
      - name: Fuzz each pack parser for 3 minutes
        run: |
          for target in packwiz_pack packwiz_index packwiz_meta mrpack_index sidecar pack_paths; do
            cargo fuzz run "$target" -- -max_total_time=180 -rss_limit_mb=2048 -max_len=65536 -timeout=10
          done
      - uses: actions/upload-artifact@v4
        if: failure()
        with:
          name: fuzz-crashes
          path: fuzz/artifacts
```

Also add `permissions: { contents: read }` at the top level of `nightly.yml` (a Plan 6 carry-forward that costs one line), and `timeout-minutes: 60` on `real-install`.

- [ ] **Step 3: Check locally without fuzzing**

Run: `cargo metadata --format-version 1 --no-deps > /dev/null && cargo test --workspace`
Expected: the workspace still resolves (the fuzz crate is excluded) and every test passes. **Do not** run `cargo fuzz` or build `fuzz/` here.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml fuzz .github/workflows/nightly.yml
git commit -m "ci(nightly): sync the real Asalith pack and fuzz the pack parsers (CI only)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

The fuzz job and the real-pack step first run in Task 21, when the branch is pushed and the nightly workflow is dispatched on it.

---

### Task 21: Verify with the real Asalith pack, owner launch, PR

The real downloads here are normal pack syncs (about 81 files), not stress tests. Every scratch folder lives in the session scratchpad (`$SCRATCH` below). Use Node scripts; there is no Python on this PC.

- [ ] **Step 1: The full gate on a clean tree**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --filter ui build && pnpm -r lint && pnpm -r typecheck && pnpm -r test
```
Expected: all green. Record the test counts for the PR.

- [ ] **Step 2: Real Asalith sync and launch dry-run from the CLI**

```bash
ROOT="$SCRATCH/pl3-real"
cargo run -q -p perland-cli -- --root "$ROOT" offline Tester
ID=$(cargo run -q -p perland-cli -- --root "$ROOT" pack add https://perlanddev.github.io/asalith-modpack/pack.toml)
cargo run -q -p perland-cli -- --root "$ROOT" launch --instance "$ID" --dry-run
cargo run -q -p perland-cli -- --root "$ROOT" sync --instance "$ID"
```
Expected:
- `pack add` prints `asalith-fields`.
- The dry run ends with `dry run OK`.
- The second `sync` prints `up to date`.

Then check:
- The jar count in `$ROOT/instances/$ID/minecraft/mods` equals the number of client and both-side entries. Compute it from the live pack's metafiles, or from `managed.json` entries under `mods/`.
- None of the four `side = "server"` mods are present.
- All four CurseForge jars are present (they came from `mediafilez.forgecdn.net`).
- `instance.json` says Forge `47.4.22`.

- [ ] **Step 3: An update test with a locally served copy**

Write `$SCRATCH/serve.mjs`:

```js
// node serve.mjs <dir> [port]: a static server for a pack folder, on 127.0.0.1 only.
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

const dir = path.resolve(process.argv[2]);
const port = Number(process.argv[3] ?? 8765);
http
  .createServer((req, res) => {
    const rel = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    const file = path.join(dir, rel);
    if (!file.startsWith(dir) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200);
    fs.createReadStream(file).pipe(res);
  })
  .listen(port, '127.0.0.1', () => console.log(`serving ${dir} on http://127.0.0.1:${port}`));
```

Write `$SCRATCH/mkpack.mjs`:

```js
// node mkpack.mjs <dir> <version>: rebuild index.toml (mods/*.pw.toml as metafiles, config/** as
// preserved files) and pack.toml's index hash and version, like `packwiz refresh`.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';

const dir = path.resolve(process.argv[2]);
const version = process.argv[3];
const sha256 = (b) => crypto.createHash('sha256').update(b).digest('hex');
const walk = (d) =>
  fs.existsSync(d)
    ? fs.readdirSync(d, { withFileTypes: true }).flatMap((e) =>
        e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)])
    : [];
const rel = (f) => path.relative(dir, f).replaceAll('\\', '/');
let index = 'hash-format = "sha256"\n';
for (const f of walk(path.join(dir, 'mods')).filter((f) => f.endsWith('.pw.toml')).sort()) {
  index += `\n[[files]]\nfile = "${rel(f)}"\nhash = "${sha256(fs.readFileSync(f))}"\nmetafile = true\n`;
}
for (const f of walk(path.join(dir, 'config')).sort()) {
  index += `\n[[files]]\nfile = "${rel(f)}"\nhash = "${sha256(fs.readFileSync(f))}"\npreserve = true\n`;
}
fs.writeFileSync(path.join(dir, 'index.toml'), index);
const pack = fs
  .readFileSync(path.join(dir, 'pack.toml'), 'utf8')
  .replace(/^version = ".*"$/m, `version = "${version}"`)
  .replace(/^hash = ".*"$/m, `hash = "${sha256(Buffer.from(index))}"`);
fs.writeFileSync(path.join(dir, 'pack.toml'), pack);
```

Then:
1. **Build v1.** Copy `E:\asalith-modpack\pack.toml` and `E:\asalith-modpack\mods\` into `$SCRATCH/pack`. Add `config/perland-test.toml` (`greeting = "v1"`) and a `launcher.toml`:
   ```toml
   [art]
   icon = "icon.svg"
   [sync]
   policy = "auto"
   ```
   Copy `E:\asalith-modpack\assets\favicon.svg` to `$SCRATCH/pack/icon.svg`.
2. **Publish v1.** Run `node $SCRATCH/mkpack.mjs $SCRATCH/pack 1.0.3-local`, then start `node $SCRATCH/serve.mjs $SCRATCH/pack` in the background.
3. **Install v1.** Run `perland-cli --root "$SCRATCH/pl3-update" pack add http://127.0.0.1:8765/pack.toml`, using the same `offline` account step as above. Check that:
   - `instance.json` has `"updatePolicy": "auto"`
   - `.perland/art/icon.svg` exists
   - `config/perland-test.toml` was written
4. **Act as the player:**
   - write `minecraft/mods/mine.jar` (any bytes)
   - change `minecraft/config/perland-test.toml` to `greeting = "mine"`
5. **Publish v2.**
   - Delete `$SCRATCH/pack/mods/appleskin.pw.toml`.
   - Change `config/perland-test.toml` to `greeting = "v2"`.
   - Run `node $SCRATCH/mkpack.mjs $SCRATCH/pack 1.0.4-local`.
6. **Update.** Run `perland-cli … sync --instance <id>`. Expected output: `1.0.3-local → 1.0.4-local: +0 added · 0 updated · 1 removed`.
7. **Check what survived:**
   - `mods/mine.jar` is still there, and `pack additions` lists it.
   - `config/perland-test.toml` still says `greeting = "mine"`.
   - The AppleSkin jar is gone from `mods/` and present under `.perland/trash/<seconds>/mods/`.
   - A second `sync` prints `up to date`.
8. Stop the server.

- [ ] **Step 4: Ask the owner for the real launch from the desktop app**

The app in 3a doesn't sync packs yet (that's Plan 3b), but it launches the instance the CLI synced. Ask the owner to run:

```bash
PERLAND_LAUNCHER_ROOT=<the $SCRATCH/pl3-real folder> pnpm tauri dev
```

Then:
1. Select "Asalith Fields".
2. Press Play.
3. Confirm that Forge loads all the mods and the title screen appears. Joining the Asalith server is optional.

Wait for their confirmation. Fix anything it finds with superpowers:systematic-debugging, adding a regression test.

- [ ] **Step 5: Rebase, push, PR, and run the nightly on the branch**

1. Bring the branch up to date with `main`: use the ccd_host `sync_with_base_branch` tool if this is an app-made worktree; otherwise run `git fetch origin && git rebase origin/main`. Resolve conflicts additively. A Plan 4 session may have touched `error.rs`, the i18n files or the contract: keep both sides, then regenerate the contract.
2. Run the full gate again.
3. Push, then open the PR against `main` with `gh pr create`. The body lists what Plan 3a ships, the owner's decisions, the verification (test counts, real Asalith numbers, the update test, the owner launch), the deferred items and what 3b will do. It ends with the attribution line.
4. Run `gh workflow run nightly.yml --ref <branch>` and watch it with `gh run watch`. Both the real-pack step and the fuzz job must pass. A fuzz crash is a real bug: download the artifact, reproduce it with a **unit test** using the crashing input (never by running the fuzzer locally), fix it, and push.

- [ ] **Step 6: Follow-ups and handoff**

- Offer the owner a PR to `PerlandDev/asalith-modpack` adding its `launcher.toml` (`[sync] policy = "auto"`, `[art] icon = "assets/favicon.svg"`, `banner = "assets/og-asalith.png"`, `[about] description`). It's outward-facing, so only with their yes.
- Update the memory note (`plan-3-pack-sync.md`): status, owner decisions and deferred items for 3b. Write the durable gotchas to Dosu:
  - the CurseForge CDN host and `+` encoding
  - the packwiz stamp choice
  - `reqwest::Error`'s Display leaking URLs

---

## Deferred to Plan 3b (desktop app)

- App commands and DTOs for `add_pack`, `check`, `preview`, `ensure`/`sync`, `import_manual`, `additions`, and optional choices. Also the contract fixtures.
- The app's run holds `InstanceLock` from sync to game exit, via `sync_locked`.
- New-instance pack link and `.mrpack` tiles.
- The Play-time "Update ready" dialog for `ask`, the instance-view banner and changelog, and "Kept your version of N files".
- Mods: managed vs "Your additions", and optional toggles.
- The "Download these manually" dialog: polls `manual::scan` on `downloads_dir()` and opens each `page_url` through the opener allow-list.
- Pack art in the marquee (served from `.perland/art/`), falling back to the generated gradient.
- `perland://pack?url=` via `single-instance` and deep-link registration (https only, through `PackInput::parse`).
- Revisit `java_test`/`mods_add` webview paths now that remote input exists.
- `ui/src/ipc/mock.ts` still returns Fabric versions for Forge (a Plan 2 leftover).

## Deferred beyond Plan 3

- Object-cache garbage collection (Plan 6 maintenance).
- Hardlinking cache objects (only if disk use becomes a complaint).
- Legacy pre-1.7 asset layouts.
- The argfile charset on Windows.
- Browse (Plan 3c).
