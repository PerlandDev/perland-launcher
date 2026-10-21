# Perland Launcher: Crash Doctor, Design

- **Date:** 2026-10-21
- **Issue:** PerlandDev/perland-launcher#87.
- **Builds on:**
  - `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (the launch, `GameExit`, the redacted log);
  - `docs/superpowers/specs/2026-10-10-launcher-desktop-ui-design.md` (the activity drawer and its crash card);
  - PerlandEngine `internal/minecraft/diagnosis` (`catalogue.go`, `testdata/rules`): the server-side rule table this mirrors. Read for its shape; the launcher's rules are written for the client's logs (see §9 on attribution).
  - `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Status:** one of four parallel launcher features (A undo last update, B crash doctor, C continue, D share). The owner is away and asked for no questions; every decision here is **our call** unless the brief states it, and each records its reason.

## 1. Why

When the game crashes, `launch/crash.rs` recognises one cause, `java.lang.OutOfMemoryError`. Every other crash shows "<name> closed unexpectedly · Exit code 1" and a Copy for support button. A casual 12–25 player can't read a Fabric resolution error or a JVM `hs_err` file, and the fix is usually one click the launcher already knows how to do: turn a mod off, raise memory, use the recommended Java, re-download files.

## 2. Outcome

When a game exits on its own with an error, the launcher reads what the game left behind and, when it recognises the cause, the crash card says it in one plain sentence with the mods named, a short next step, and one button per fix it can do itself:

| Cause | Card says (en) | Buttons |
|---|---|---|
| Out of memory | Minecraft ran out of memory. | **Raise to 6 GB**, or **Memory settings** when there's no room |
| Graphics driver crash | Your graphics driver crashed the game. | **Update drivers** (the vendor's page) when the vendor is known |
| Java too old | This game needs Java 21 or newer. | **Use the recommended Java** when that's enough, else **Java settings** |
| Java too new | This version of Minecraft needs an older Java. | **Use the recommended Java**, else **Java settings** |
| Missing dependency | Create needs Flywheel, which isn't installed. | **Find Flywheel** (Browse, only when it's on Modrinth) |
| Two mods clash | Sodium and OptiFabric don't work together. | **Turn off OptiFabric**, **Turn off Sodium** |
| Mod for another version | Xaero's Minimap is made for a different version of Minecraft. | **Turn off Xaero's Minimap** |
| Mod failed while loading (mixin) | Create broke while the game was loading. | **Turn off Create** |
| Damaged mod file | The file for Sodium is damaged. | **Turn off Sodium** |
| Damaged game files | Some game files are damaged. | **Repair and play** |
| Anything else | today's card, unchanged | Copy for support |

Copy for support stays on every card. A "Turn off" button is only offered for a mod the player added (a pack's own mods are the pack's, `refuse_managed`) and only when the jar was found on disk.

## 3. Goals, non-goals, anti-goals

- **Goals:** the rule table and its fixtures; reading the four evidence sources; the typed cause with the mods named; the fixes as a list with one primary; the card; en and es; working in `dev:mock` (one mock instance per cause behind `?crash=<code>`).
- **Non-goals:**
  - a downloadable Java the game's version doesn't ship (Java handling stays as it is: automatic Mojang runtime or the player's pick);
  - repairing a pack's own mod files (the pack sync owns them; a damaged pack mod gets the cause and no button);
  - "suspected mods" heuristics from crash-report stack frames (too often wrong to put behind a button);
  - diagnosing a launch that never started (that is already a `LauncherError` card);
  - uploading logs anywhere.
- **Anti-goals:**
  - a button that claims a fix it can't do (never fake what isn't built: no Browse button for a mod that isn't on Modrinth, no Turn off for a pack mod);
  - a wrong cause: a near-miss must stay generic, so every rule has a near-miss fixture;
  - a second lit button on Home: the card's fixes are raised, never amber; Play stays the one lit control;
  - a token, the home folder or the invite token in anything the card shows or stores.

## 4. Evidence (our call)

Read only after an exit the launcher calls a crash (`!killed && code != 0`), from the instance's game folder (`LaunchCommand::cwd`), each capped so a 2 GB log can't stall the launcher, and each passed through the run's `Redactor` before any rule sees it:

| Source | Which file | Read |
|---|---|---|
| output tail | the last 200 lines the launcher kept (already redacted) | all |
| crash report | the newest `crash-reports/crash-*.txt` modified at or after the launch | first 256 KiB |
| game log | `logs/latest.log` if modified at or after the launch | last 512 KiB |
| JVM fatal error log | `hs_err_pid<pid>.log` (the game's own pid) | first 128 KiB |

Files older than the launch belong to an earlier run and are ignored (a stale crash report would name the wrong cause). Reading happens in the spawn task after the output drained, in `spawn_blocking`, and never fails the exit: an unreadable file is skipped.

## 5. The rule table (our call on order and wording)

`crates/launcher-core/src/launch/crash/` becomes a module: `evidence.rs` (§4), `rules.rs` (the table), `mods.rs` (finding the jar a mod id or mixin config belongs to), `testdata/` (fixtures). Each rule has a stable code, regex patterns tried in order over each source, and a function from the named groups to a typed `CrashCause`. Rules run **in order; the first that matches any source wins**, so a cause comes before its consequence:

1. **`crash_out_of_memory`**: `java.lang.OutOfMemoryError` with `Java heap space`, `GC overhead limit exceeded`, `Metaspace`, `Direct buffer memory` or no detail. Near-miss: `unable to create native thread` (raising memory doesn't fix it). First, because running out of memory mid-load produces mixin and class errors downstream.
2. **`crash_graphics_driver`**: an `hs_err` whose problematic frame is a GPU driver (`atio6axx`, `atioglxx`, `amdxx`; `nvoglv32/64`, `libnvidia-glcore`; `ig*icd32/64`), or `Pixel format not accelerated`, or GLFW's `WGL: The driver does not appear to support OpenGL`. Param `vendor` (`amd`, `nvidia`, `intel`) when the frame names one. A native crash isn't a consequence of a mod error, so it comes before them.
3. **`crash_java_too_old`**: `UnsupportedClassVersionError … (class file version N) … up to M` (required = N − 44), or Fabric's `requires version N or later of '…' (java)`. Params `required`, and `running` when known.
4. **`crash_java_too_new`**: old Forge's `ClassLoaders$AppClassLoader cannot be cast to … java.net.URLClassLoader`.
5. **`crash_mod_missing_dependency`**: Fabric/Quilt `Mod 'Name' (id) v requires … of 'Dep' (dep), which is missing!` (and `… of mod 'dep'` forms), Forge/NeoForge `Missing or unsupported mandatory dependencies:` then `Mod ID: 'dep', Requested by: 'id'` with `Actual version: '[MISSING]'`. Params `mod`, `missing`. A Forge dependency on `minecraft`, `forge` or `neoforge` is not a missing mod: it is rule 7.
6. **`crash_mods_clash`**: Fabric `Mod 'A' (a) v is incompatible with … mod 'B' (b)`. Params `mod`, `other`.
7. **`crash_mod_wrong_version`**: Fabric `Mod 'A' (a) v requires … of 'Minecraft' (minecraft)` / `… of 'Fabric Loader' (fabricloader)`, and Forge `Mod ID: 'minecraft'|'forge'|'neoforge', Requested by: 'a'`. Param `mod`.
8. **`crash_mod_failed`**: `Mixin apply failed <config>` / `Mixin apply for mod <id> failed <config>` / `Mixin transformation of … failed` naming a config. Param `mod` (the mod that owns the config, found by §6). Without a config that resolves to a mod, it doesn't match (a mixin error without a culprit is generic).
9. **`crash_mod_file_damaged`**: `ZipException`/`invalid LOC header`/`zip END header not found`/`Invalid or corrupt jarfile` on a line naming `mods/<file>.jar`. Param `mod` (file name).
10. **`crash_game_files_damaged`**: the same zip errors naming anything else, `Could not find or load main class` / `ClassNotFoundException` for the game's or loader's entry class (`net.minecraft.client.main.Main`, `net.fabricmc.loader.impl.launch.knot.KnotClient`, `org.quiltmc.loader.impl.launch.knot.KnotClient`, `cpw.mods.bootstraplauncher.BootstrapLauncher`, `net.minecraftforge.bootstrap.ForgeBootstrap`, `net.minecraft.launchwrapper.Launch`).

Anything else: no cause; the card is today's generic card.

Rules 8 to 10 skip a line logged below error level (`[thread/WARN]`, `[thread/INFO]`): Mixin and the loaders log a non-required mixin's failure, or a jar they skip, as a warning and carry on, so blaming that mod for a crash with another cause would put a wrong "Turn off" on the card (**our call**, after review). A damaged jar is named by its file and gets Turn off only when it is an enabled jar in `mods/`.

Patterns are written from what the loaders really print (Fabric Loader 0.14–0.16 `ModResolutionException` text, Quilt's equivalent, FML 47/NeoForge 20.x `ModSorter`/`ModLoadingIssue`, HotSpot `hs_err` headers, Mixin 0.8). **Wrong-loader jars:** a Forge mod in a Fabric instance (or the reverse) is ignored by the loader without an error, so there is nothing to read; when it does break something it surfaces as rule 5 or 8.

## 6. Naming the mods (our call)

Logs give mod ids (and Fabric gives display names). To name and turn off a mod, `crash::mods` scans the enabled `mods/*.jar` once per diagnosis (zip central directory only; metadata files capped at 1 MiB like `mod_icons`):
- id and name from `fabric.mod.json` (`id`, `name`), `quilt.mod.json` (`quilt_loader.id`, `quilt_loader.metadata.name`), `META-INF/mods.toml` / `META-INF/neoforge.mods.toml` (`[[mods]] modId`, `displayName`);
- a mixin config is matched by the jar that contains that entry at its root.

The cause carries, per mod, `{ id, name, file? }`: the name from the jar, else from the log, else the id. `file` only when the jar was found. Params put into the player's message are names; files are only in the fix.

## 7. The model (our call)

**Core** (`launcher-core`, no app context): `GameExit.crash: Option<Crash>` where `Crash { cause: CrashCause, detail: String }`. `CrashCause` is a typed enum (one variant per §5 rule, carrying `ModRef`s, `required`/`running`, `vendor`). `Crash::error()` builds the player-facing `LauncherError`: a new `ErrorCode::Crash*` per rule, the English message and fix, `params` (`mod`, `other`, `missing`, `required`, `running`, `vendor`), and `detail` = the matching lines (already redacted). The UI translates by code like every other error; the English is the fallback.

**App** (`launcher-app`): `ExitInfo` gains

```text
cause: Option<LauncherError>   // replaces `crash: Option<CrashHint>`
fixes: Vec<CrashFix>           // offered on the crash card, first is the primary
```

`CrashFix` is a tagged enum (`kind`), built from the cause and the instance:

| kind | when | does |
|---|---|---|
| `more_memory` | out of memory | UI: raise by 2 GB within ¾ of the computer's memory (`suggestMemory`, unchanged), or open instance settings when there's no room |
| `recommended_java` | java too old/new, the instance has a custom Java, and the recommended Java (`java_required(mc)`) is new enough (too old) or the cause is too new | sets `java.path` to automatic |
| `java_settings` | java cause otherwise | opens instance settings |
| `find_mod { project }` | missing dependency | UI looks the id up with `browse_project`; shows **Find <title>** only on success, which opens Browse on that project for this instance |
| `disable_mod { file, name }` | clash (both mods, the named-second first, matching Fabric's own "Remove mod 'B'" advice), wrong version, mod failed, damaged mod file; only for a found jar the pack doesn't own | `mod_set_enabled(file, false)` |
| `repair` | damaged game files | a run that re-verifies every game file by hash (`ExistingCheck::Hash`) and plays |
| `update_drivers { vendor }` | graphics driver with a known vendor | opens the vendor's driver page (AMD, NVIDIA, Intel), https via `open_link` |

Two fields, not one struct, so Session A's "Go back to how it was" is just another `CrashFix` kind (`restore`) pushed onto `fixes`, also on a crash with no known cause.

**Repair** is `AppCore::play_repair(id)` (command `play_repair`): `start_run` with `PlayArgs { repair: true }`; `Runner::prepare` gets `repair: bool`, and `CoreRunner` prepares with an installer whose downloader checks existing files by hash. Everything else about the run is the same (sync, Play, progress). Hashing alone can't see a damaged Forge/NeoForge processor output (they are skipped once `.perland-loader` exists, and NeoForge's list no output hashes), and that is exactly what makes `BootstrapLauncher` fail to load, so a repair also drops that marker first and the processors run again (**our call**: otherwise the button would sometimes claim a fix it can't do).

## 8. The card (our call; through `/impeccable`)

- Title: the cause's message (translated), else "<name> closed unexpectedly". Body: the cause's fix line, else "Exit code N".
- Fixes: one row of raised keys, the first the primary, the rest ghost; then a ghost Copy for support. Never amber. A fix button that acts (turn off, Java, memory) dismisses the card on success and says what changed in a toast ("Turned off Sodium. Press Play to try again."). A failure is the usual error toast.
- "Show details" (`ErrorCard`'s existing detail toggle) reveals the matching log lines, already redacted.
- Copy for support adds the cause's code and detail above the log tail.

## 9. Constraints

- `launcher-core` and `launcher-app` stay free of Tauri; `src-tauri` gets one command (`play_repair`), wiring only.
- `regex` (already in `Cargo.lock` through `tracing-subscriber`) becomes a direct dependency of `launcher-core`.
- Every new string in `en.json` and `es.json`; every new `ErrorCode` has `errors.<code>.message` and `.fix` in both.
- The IPC contract is regenerated, never hand-edited.
- **Attribution:** the launcher's rule table is written fresh for client logs in Rust; it ports no PerlandEngine code (the engine's Go entry parser, subjects and fingerprints are server concepts the launcher doesn't need). Both projects are PerlandDev's. Where a pattern reads the same loader message as an engine pattern, the module header says it mirrors `internal/minecraft/diagnosis/catalogue.go`, and `NOTICE` lists it, so the provenance is on record either way.
- Fixtures: `testdata/<code>.hit.txt` and `<code>.miss.txt`, cases split by `---`, headers `# source: tail|crash_report|latest_log|hs_err` and `# param k=v`. A test asserts every rule has both files, every hit case yields its code and params, and every miss case doesn't yield that code. Tiny, written from the real formats. No fuzzing locally.

## 10. Settled after the builder check

- **Parallel sessions.** B starts from `main` and owns the card layout. The fix row renders with or without a cause, so Session A's `restore` needs no cause; whichever of A and B merges second adapts by merging `main` (never dropping the other's work). The shape was sent to the coordinating session on 2026-10-21.
- **Repair and Forge processors.** See §7: a repair drops the processor marker.
- **What counts as "on Modrinth" for Find.** The id is first mapped through a small alias list (Fabric API's module ids, `fabric` and `fabric-api-base`/`fabric-*-v1` style ids → `fabric-api`; `quilted_fabric_api` → `qsl`), then looked up with `browse_project`; Find shows only when that returns a project whose `kind` is `mod`. It opens the project page, which already shows whether a version fits this instance and installs through the normal flow (refusing an incompatible one with its own error), so the button never promises an install, only the page. Checking versions on the card would duplicate that flow (**our call**).

## 11. Left to the builder

Exact regexes; the es wording within the rules above; the mock's per-cause demo; how the card lays out the key row at 960 × 600.
