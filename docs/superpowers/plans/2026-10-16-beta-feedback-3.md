# Closed-beta feedback, round 3 (#32, #36, #37, #38) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Answer the third round of Discord reports from the closed-beta tester:
- a playtime counter per instance and in total (#32);
- background position and size guidance, a real gear for Settings, and a round icon shape (#37);
- importing a whole instance from another launcher, worlds and mod data included (#38).

**Architecture:** One PR off `main` on branch `claude/gifted-shannon-e7veg2`.
- **Core:**
  - `playtime.rs`: a per-instance `.perland/playtime.json`;
  - `Instance.banner_focus`;
  - `Settings.icon_shape`;
  - `import/`: read another launcher's instance folder and copy its game folder.
- **App:**
  - `drive` records the seconds played;
  - the commands `playtime_list`, `instance_banner_focus_set`, `import_inspect`, `import_instance` and `import_cancel`.
- **UI:**
  - playtime on Home and Accounts;
  - a position control in "Change image…";
  - the gear;
  - the icon shape setting;
  - a fourth New instance mode, "From another launcher".

**Tech Stack:** Rust (tokio, serde), Tauri 2.12, React 19, TanStack Query, zod, Vitest, Testing Library, i18next.

**Spec:** this document. The owner handed every call to the implementer on 2026-10-16 ("take recommended decisions yourself, let me know when in doubt"). Every decision marked **(our call)** is for the owner to review on the PR. Prior art: `docs/superpowers/plans/2026-10-16-beta-feedback-2.md`.

## Decisions

### #36: integrated skin editor (our call: not now)
- Changing the official skin needs a Microsoft sign-in and the Minecraft services API.
- Microsoft sign-in is not built (PRODUCT.md, "Not built: Microsoft sign-in (Plan 4)"), and the core has no MSA code.
- **Nothing is built in this round.** The issue stays open as input for Plan 4. A reporter update says so.

### #32: achievements, playtime, medals (our call: playtime now, the rest deferred)
- **Playtime is built.**
  - **What counts:** the launcher measures each session itself, from the moment the game is `Running` until the process exits (a normal exit, a crash or Stop).
  - **Old sessions:** time played before this build, or in other launchers, is not counted.
  - **Storage:** `instances/<id>/.perland/playtime.json`, holding `{ "seconds": u64, "lastPlayed": u64 }` (Unix seconds), written with `write_atomic`.
    - It is a separate file, not an `Instance` field, so `instance_update` from a stale UI copy can never reset it.
    - It moves with the folder on rename and goes away on delete.
  - **When it is written:** after the game exits, before `finish`, inside `drive`. The write never fails the run: an error is only logged.
  - A launcher that dies mid-session records nothing for that session. A periodic checkpoint is deferred.
  - **Where it shows (our call):**
    - **Home:** the version plate gains "· 12 h played". It is hidden under 1 minute.
    - **Accounts:** under the account list, one line: "Time played on this computer: 3 days 4 h". It is the sum over all instances and is hidden when that sum is 0.
  - **Format (`formatPlaytime(seconds, t)`):**
    - under 1 h: `{m} min`;
    - under 24 h: `{h} h {m} min`, dropping `0 min`;
    - under 365 days: `{d} days {h} h` (plural-aware), dropping `0 h`;
    - otherwise: `{y} years {d} days`.

    Spanish uses `min`, `h`, `día/días` and `año/años`.
- **Achievements: deferred.**
  - On a server, Minecraft keeps advancements on the server, not in the instance.
  - Perland players mostly play on servers, so a launcher-side list would be empty or wrong for them.
  - Doing this properly needs server-side support, which is a separate project.
- **Medals: deferred.** They depend on achievements and on a design decision about what earns one.

### #37: customization (our call per item)
1. **Background position and size guidance: built.**
   - **Model:** `Instance.banner_focus: Option<Focus>`, where `Focus { x: u8, y: u8 }` holds percentages 0–100 (camelCase `bannerFocus`, `null` = default).
     - The default keeps today's `center bottom`, which is `{ x: 50, y: 100 }`.
     - It applies to a **picture** background only (an upload or pack art). The generated landscapes are drawn scenes and ignore it.
   - **Command:** `instance_banner_focus_set(id, focus: Option<Focus>) -> Instance`.
     - It refuses values over 100 with `banner_focus_invalid`.
     - Busy and installing refusals match `instance_art_set`.
     - `instance_art_set` (any background change) resets `banner_focus` to `None`, because a position chosen for one picture means nothing for the next.
     - `AppCore::instance_update` keeps `banner_focus` from disk, like `art` and `icon`.
   - **UI:** in "Change image…", under the Background keys, a **Position** control appears while a picture background is in use.
     - It is a small framed preview of the whole picture with a crosshair at the focus. Click or drag moves it, the arrow keys move it by 5, and a "Center" ghost button resets it.
     - It saves when the pointer is released or after a key press (debounced 300 ms). The marquee behind the dialog follows live.
     - The marquee's `<img>` gets `object-position: {x}% {y}%`.
   - **Size guidance:** the Background upload hint becomes "PNG, JPEG or WebP, up to 5 MB. Best at 1920 × 1080 or larger." The Icon upload hint becomes "PNG, JPEG or WebP, up to 5 MB. Square works best."
   - **Zoom is deferred.** Cover-fit plus a focus point covers "choose the visible area" for most pictures.
2. **Interface colours and font: not built, owner question.** PRODUCT.md binds a single dark, game-forward look in which every hue has a role: amber means act, lime means OK, coral means fault and blue means info. Free colour and font choice would break those roles, and the owner chose that direction. This is flagged to the owner.
3. **Settings icon: built.** Pixelarticons' `Gear` reads as a chip, so the title bar uses `SettingsCog` (same set, a real cog).
4. **Icon shape: built.**
   - `Settings.icon_shape: IconShape` is `Square` (the default) or `Round` (camelCase `iconShape`, values `"square"`/`"round"`). A bad value falls back to the default like every other setting.
   - **Settings screen:** a segmented choice, "Instance icons: Square / Round".
   - **Round** is a pixel-stepped circle (a stepped `clip-path` polygon on the 1/16 grid), so it stays in the pixel language.
   - It applies to the rail's icon tiles, compact and wide. The Home icon plate and the picker keep their frames.

### #38: import an instance from another launcher (our call on the whole design)
- **Entry point:** a fourth New instance mode, "From another launcher" ("De otro launcher").
  - **Step 1:** "Choose folder…" opens a directory picker, with the hint "In your other launcher, open the instance's folder (usually right-click → Open folder) and pick it here."
  - **Step 2:** a form prefilled from what was detected: Name, Minecraft version and loader (`VersionFields` with an initial value), and a line "We'll copy {{size}}: your mods, worlds, settings and map data. The original folder isn't changed."
  - **Step 3:** "Import" (the lit button). While it copies, a segmented meter shows MB done and total, with a Cancel button. On success it navigates home to the new instance.
- **Detection (`launcher_core::import::inspect(dir)`), tried in this order:**
  1. **Prism Launcher / MultiMC.**
     - **Marker:** `instance.cfg` plus `mmc-pack.json`.
     - **Name:** `name=` from `instance.cfg`.
     - **Components by `uid`:**
       - `net.minecraft` gives the Minecraft version;
       - `net.fabricmc.fabric-loader` gives Fabric;
       - `org.quiltmc.quilt-loader` gives Quilt;
       - `net.minecraftforge` gives Forge;
       - `net.neoforged` gives NeoForge.
     - **Game dir:** `.minecraft/`, otherwise `minecraft/`.
  2. **CurseForge.**
     - **Marker:** `minecraftinstance.json`.
     - **Fields:** `name`, `gameVersion`, and `baseModLoader.name`, which is `forge-47.2.0`, `neoforge-20.4.80-beta`, `fabric-0.15.11-1.20.1` or `quilt-0.26.0-1.20.1`. The loader is the prefix before the first `-`, and the version is the next part.
     - **Game dir:** the folder itself.
  3. **Modrinth App (legacy).**
     - **Marker:** `profile.json` with `metadata`.
     - **Fields:** `metadata.name`, `metadata.game_version`, `metadata.loader` (`vanilla`/`fabric`/`quilt`/`forge`/`neoforge`) and `metadata.loader_version.id`.
     - **Game dir:** the folder itself.
  4. **ATLauncher.**
     - **Marker:** `instance.json` with a `launcher` object.
     - **Fields:** `launcher.name`, top-level `id` (the Minecraft version), and `launcher.loaderVersion.type` (Fabric/Quilt/Forge/NeoForge, case-insensitive) with `.version`.
     - **Game dir:** the folder itself.
  5. **Any other Minecraft folder.**
     - **Marker:** at least one of `mods/`, `saves/`, `config/`, `resourcepacks/`, `shaderpacks/` or `options.txt`. This covers the newer Modrinth App, the official launcher's `.minecraft` and others.
     - **Detected:** nothing, so the player picks the version and loader.
     - **Game dir:** the folder itself.
  - **The player picked the inner game folder:** if the picked folder is named `.minecraft` or `minecraft` and its parent matches rule 1, the parent is used.
  - **Nothing matches:** `import_not_instance`.
  - **Loader mapping:** a loader the launcher doesn't support, or a malformed field, leaves that part undetected (`None`) instead of failing. The player picks it.
- **Refusals:**
  - `import_inside_launcher`: the picked folder is inside the launcher's data root, or contains it. Copying would recurse or eat our own files.
  - `import_not_instance`: see above.
- **What is copied:** everything under the game dir, recursively, except:
  - **these top-level entries (case-insensitive):** `assets`, `libraries`, `versions`, `natives`, `runtime`, `.fabric`, `.quilt`, `.cache`, `logs`, `webcache`, `webcache2`, `treatment_tags.json`, and every entry whose name starts with `launcher_` (`launcher_profiles.json`, `launcher_accounts*.json`, `launcher_msa_credentials*.bin`, `launcher_settings.json`, `launcher_entitlements.json`, the `launcher_*log*.txt` logs). The launcher installs its own copies of the first three and its own Java runtime, and the rest are caches or another launcher's account data, credentials, settings and logs;
  - **when the game dir is the picked folder itself, also:** `instance.cfg`, `mmc-pack.json`, `minecraftinstance.json`, `profile.json`, `instance.json` and `.curseclient`;
  - **symlinks:** never followed, never copied.

  Everything else is copied: `mods/` (including `mods/.index`), `config/`, `saves/`, `resourcepacks/`, `shaderpacks/`, `screenshots/`, `XaeroWaypoints/`, `xaero/`, `journeymap/`, `options.txt`, `servers.dat` and so on.
- **Never touch the source:** it is only read.
- **How the copy runs:**
  1. Copy into a staging folder `<root>/import-staging/<n>/` (same volume as `instances/`, and not listed, because it is outside `instances/`).
  2. On success, `InstanceStore::create(name, game)`, then replace the new, empty `minecraft/` with the staged folder by `rename`.
  3. On failure or cancel, delete the staging folder. No instance appears.

  `import-staging/` left by a crash is removed at the next import.
- **The resulting instance:** a plain instance (`SourceRef` as `instance_create` makes it) with the default memory from settings. Imported jars show as local mods.
  - **Deferred:** carrying over the other launcher's memory, Java arguments and icon.
- **Progress:** `AppEvent::Progress { instance: None, task: TaskId::new("import_instance"), done, total, bytes }`, with bytes copied and total bytes, throttled like other progress.
  - The UI routes `import_instance` like `install_content`, into its own small store, never onto Play.
  - **Cancel:** `import_cancel()` cancels the one running import. A second `import_instance` while one runs → `import_busy`.
- **Size:** `import_inspect` returns `size_bytes`, summed over exactly the files that would be copied.
- **No brand names in the UI copy:** the hint speaks of "your other launcher". Detection is silent; it only prefills the form.

## Execution notes (2026-10-16)

- **Shipped:** PerlandDev/perland-launcher#39 (draft). It refs #32, #36 and #37 and fixes #38. #36 has no code: it waits for Microsoft sign-in.
- **Owner question still open:** #37.2, interface colours and fonts.
- **Calls made during execution, beyond the Decisions above:**
  - Spanish `home.played` is "{{time}} de juego". "jugadas" doesn't agree with "días" or "años".
  - The Position control is a labelled group with two visually hidden range inputs plus a pointer surface. Its reset button says "Reset" / "Restablecer", because the default is bottom-centre, not the centre.
  - The import copy's progress callback is `Sync`, so the future is `Send`.
  - The import refuses a picked folder that is a link, and a linked game folder.
  - It skips `session.lock` anywhere in the tree, and every top-level `launcher_*` entry, `webcache`, `webcache2`, `runtime` and `treatment_tags.json`.
  - It skips a source file that vanishes mid-copy.
  - It checks free space first (`not_enough_space`).
  - It retries the final move for about 2 s while Windows holds the folder.
  - "Restart now" waits for an import.
  - A player's cancel isn't logged as an error.
  - A folder with nothing detected is named after its parent when it is `.minecraft` or `minecraft`. Names are capped at 64 characters.
  - Leaving the import form doesn't cancel the import. It continues in the background; coming back shows its progress, success shows a notice and failure an error toast.
  - Quitting with "keep playing" records the running games' time so far.
  - An unknown `iconShape` loads as Square instead of resetting every setting.
- **Deferred:**
  - **Import:**
    - carrying over memory, Java arguments and the icon from the other launcher;
    - cancel only takes effect between files, so one huge file delays it;
    - New instance opens on its default mode even while an import runs;
    - instance rename still has no Windows in-use retry.
  - **Position control:**
    - the drag preview is written into the shared listing cache, so an unrelated refetch mid-drag can show the old position on the marquee until the next refetch;
    - moving the other axis with the arrow keys isn't announced to screen readers.
  - **Playtime:**
    - nothing is checkpointed during a session, so a launcher crash mid-game loses that session;
    - damaged instances drop out of the total.
  - **Round icons:** the round tile sits on the square cabinet plate.

## Global Constraints

- `crates/launcher-core` and `crates/launcher-app` never depend on Tauri. `src-tauri` is wiring only and has no tests.
- TDD: Rust unit tests in `#[cfg(test)]`; UI uses Vitest + Testing Library. Keep test inputs tiny (a few bytes per file); never run fuzz, stress or load tests locally.
- Every UI string goes in `ui/src/i18n/en.json` and `ui/src/i18n/es.json` (neutral Latin American Spanish, informal "tú"). Every new `ErrorCode` needs `errors.<code>.message` (and `.fix` where there is one) in both. Params carry only the player's own text or numbers, never English.
- `ui/src/ipc/contract/*.json` is regenerated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, never hand-edited. `ui/src/ipc/schema.ts` uses `z.strictObject`, so a new Rust field needs its zod field in the same task. `ui/src/ipc/mock.ts` gains every new command, so `pnpm --filter ui dev:mock` works.
- UI styling goes through `/impeccable` (Skill `impeccable:impeccable`), against `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`:
  - minimum window 960 × 600;
  - Spanish runs about 30% longer;
  - at rest, only one button per view is lit;
  - hard offset steps, never blurred shadows;
  - pixel-stepped corners.

  Update the matching `DESIGN.md` paragraphs.
- No Modrinth or Mojang branding or assets, and no other launcher's name in UI copy.
- Tokens never reach logs.
- The commit trailer is exactly:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01EsufyZzqVYmAhJ77w9KHGZ
  ```
- Run the full gate before every task-ending commit:
  - `pnpm --filter ui build`
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `pnpm -r lint`
  - `pnpm -r typecheck`
  - `pnpm -r test`
- PRs #25, #26 and #27 are open (they touch `error.rs`, the i18n files, `instances.rs`, `packs.rs` and the release workflow). Keep edits there additive and small.

## Review Focus

1. **Import never harms the source or the launcher.**
   - The source folder is byte-identical after an import, a failure and a cancel.
   - Picking the launcher's own root, an instance folder inside it, or a parent of it is refused before any copy.
   - Task 4 tests all three.
2. **A cancelled or failed import leaves nothing behind.** That means no instance in the listing and no staging folder. Task 5 tests a cancel mid-copy and a failure with a fake unreadable file.
3. **Symlinks inside the source** (a Prism instance with a linked `saves/`) are skipped, not followed. On Unix, Task 4 tests a symlink to a folder outside the source.
4. **Playtime survives a stale UI copy and a rename.**
   - `instance_update` from an old copy can't change it, because it lives in its own file.
   - A rename keeps it, because the folder moves.
   - Task 2 tests the rename.
5. **A stale UI copy can't clear `banner_focus`.** `instance_update` keeps it from disk, and a new background resets it. Task 3 tests both.

---

### Task 1: The settings gear and the icon shape (#37.3, #37.4)

**Files:**
- Modify:
  - `crates/launcher-core/src/settings.rs`: `IconShape`, `Settings.icon_shape`, the default and the round-trip test;
  - `crates/launcher-app/tests/contract.rs`: the settings sample gets `icon_shape: IconShape::Round`; then regenerate;
  - `ui/src/ipc/schema.ts`: `iconShape: z.enum(['square','round'])`;
  - `ui/src/ipc/mock.ts`: the option and the initial state;
  - the literals in `ui/src/state/handleEvent.test.ts`;
  - `ui/src/ui/Icon.ts`: export `SettingsCog`;
  - `ui/src/features/titlebar/TitleBar.tsx`: `Gear` → `SettingsCog`;
  - `ui/src/features/settings/Settings.tsx` (+ test);
  - `ui/src/features/rail/Rail.tsx` (+ test);
  - `ui/src/styles/app.css`;
  - the en/es i18n files;
  - `DESIGN.md` (the Rail paragraph).

**Interfaces:**
- Produces:
  - `IconShape { Square, Round }`, with `#[serde(rename_all = "lowercase")]` and `#[default] Square`;
  - `Settings.icon_shape` (camelCase `iconShape`);
  - in the UI, `settings.iconShape`.

- [ ] **Step 1: Write the failing tests.**
  - `settings.rs`: `icon_shape_defaults_to_square_and_round_trips`. A file without the key → `Square`, and `Round` saves as `"iconShape": "round"`.
  - `Settings.test.tsx`: `picks the round icon shape` clicks "Round" in the "Instance icons" group, and the last `settings_set` carries `iconShape: 'round'`.
  - `Rail.test.tsx`: `round icon shape marks the rail` renders with `iconShape: 'round'`, and the rail root has `data-icon-shape="round"`.
  - `TitleBar.test.tsx`: the Settings button still has its accessible name, and the rendered svg is the cog (assert on a `data-icon="settings-cog"` attribute you pass through, or snapshot the path's first command).
- [ ] **Step 2:** Run `cargo test -p launcher-core settings` and `pnpm --filter ui test -- Settings Rail TitleBar`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - The Settings screen gets a segmented choice like New instance's mode `seg` buttons, placed after "Minimize while playing". Copy:
    - `settings.iconShape`: "Instance icons" / "Íconos de instancias";
    - `settings.iconSquare`: "Square" / "Cuadrados";
    - `settings.iconRound`: "Round" / "Redondos".
  - **Rail:** a `data-icon-shape` attribute on the rail root. The CSS clips `.rail-art` and the monogram tile to a stepped circle polygon on a 16-step grid, via `/impeccable`. The selection and playing marks must stay visible (they sit outside the clipped tile).
- [ ] **Step 4:** Regenerate the contract, then run the full gate. Expected: PASS.
- [ ] **Step 5:** Commit `feat: a real settings cog and a round instance icon option (#37)`.

### Task 2: Playtime (#32)

**Files:**
- Create: `crates/launcher-core/src/playtime.rs` (registered in `lib.rs`), `ui/src/ui/playtime.ts` (+ `playtime.test.ts`)
- Modify:
  - `crates/launcher-app/src/runs.rs`: the start instant at `Running`, and the record after `game.wait()`;
  - `crates/launcher-app/src/app.rs` (or a new `crates/launcher-app/src/playtime.rs`): `playtime_list`;
  - `crates/launcher-app/src/dto.rs`: `PlaytimeEntry`;
  - `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`;
  - `crates/launcher-app/tests/contract.rs`: a `playtime` fixture;
  - `ui/src/ipc/schema.ts`, `ui/src/ipc/commands.ts`, `ui/src/ipc/mock.ts`;
  - `ui/src/state/queries.ts`: `usePlaytime()` on `qk.playtime`;
  - `ui/src/state/handleEvent.ts`: invalidate `qk.playtime` on run idle;
  - `ui/src/features/home/Home.tsx`, `ui/src/features/accounts/Accounts.tsx` (+ tests);
  - the en/es i18n files.

**Interfaces:**
- Produces:
  - `launcher_core::playtime::{Playtime { seconds: u64, last_played: u64 }, read(perland_dir: &Path) -> Playtime, add(perland_dir: &Path, seconds: u64, now: u64) -> Result<Playtime>}`. Serde is camelCase. A missing or unreadable file reads as `Playtime::default()`.
  - `AppCore::playtime_list(&self) -> Result<Vec<PlaytimeEntry>>`, where `PlaytimeEntry { id: String, seconds: u64, last_played: u64 }` (camelCase) covers every listed instance, zeros included.
  - The Tauri command `playtime_list`.
  - `commands.playtimeList()` and `usePlaytime()`.
  - `formatPlaytime(seconds: number, t: TFunction): string`.

- [ ] **Step 1: Write the failing tests.**
  - `playtime.rs`:
    - `missing_file_reads_zero`;
    - `add_accumulates`: `add(60, now=100)`, then `add(30, now=200)` → `{ seconds: 90, last_played: 200 }`, and the file holds `"lastPlayed": 200`;
    - `damaged_file_restarts_from_zero`: `"nope"` in the file → `add(5)` gives 5.
  - `runs.rs` (with the existing fake runner):
    - `a_finished_game_adds_its_playtime`: the fake game runs about 1.1 s (or the clock is injected; prefer an injected `fn() -> Instant`/elapsed source if the fake allows), and after `Idle`, `playtime::read` shows `seconds >= 1`;
    - `a_game_that_never_started_adds_nothing`: a prepare failure leaves `seconds == 0`.
  - `app.rs`:
    - `playtime_follows_a_rename`: record 120 s, rename the instance → `playtime_list` shows 120 under the new id.
  - `playtime.test.ts`: `formatPlaytime`:
    - `30` → `''` (hidden);
    - `59*60` → `59 min`;
    - `3600` → `1 h`;
    - `3*3600+5*60` → `3 h 5 min`;
    - `2*86400+4*3600` → `2 days 4 h`;
    - `86400` → `1 day`;
    - `400*86400` → `1 year 35 days`;
    - Spanish `2*86400` → `2 días`.
  - `Home.test.tsx`: with the mock playtime at 12 h, the version plate reads `… · 12 h played`; at 0 there is no "played".
  - `Accounts.test.tsx`: the total line shows the sum of two instances, and it is absent at 0.
- [ ] **Step 2:** Run the named tests. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `drive` keeps `let started = Instant::now()` in the `Running` block, and after `game.wait()` calls `playtime::add(&perland_dir, started.elapsed().as_secs(), unix_now)`. An error goes to `tracing::warn!` only.
  - Copy:
    - `home.played`: "{{time}} played" / "{{time}} jugadas". Spanish reads "12 h jugadas"; the implementer may choose "Jugado: {{time}}" if the plural reads oddly, and must test that copy;
    - `accounts.totalPlayed`: "Time played on this computer: {{time}}" / "Tiempo jugado en esta computadora: {{time}}";
    - the units via i18next plurals: `time.minutes` "{{count}} min", `time.hours` "{{count}} h", `time.days_one`/`_other` "{{count}} day(s)" / "{{count}} día(s)", `time.years_one`/`_other` likewise.
  - Styling via `/impeccable`: the playtime rides the existing plate in the same voice, never a new badge.
- [ ] **Step 4:** Regenerate the contract, then run the full gate. Expected: PASS.
- [ ] **Step 5:** Commit `feat: count the time played per instance and in total (#32)`.

### Task 3: Background position and size hints (#37.1)

**Files:**
- Modify:
  - `crates/launcher-core/src/instances.rs`: `Focus`, `Instance.banner_focus`, and `create` sets `None`;
  - `crates/launcher-core/src/error.rs`: `BannerFocusInvalid`;
  - `crates/launcher-app/src/packs.rs`: `instance_banner_focus_set`, and `instance_art_set` resets the focus;
  - `crates/launcher-app/src/app.rs`: `instance_update` keeps it from disk;
  - `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`;
  - `crates/launcher-app/tests/contract.rs`: give `fabric_instance` `banner_focus: Some(Focus { x: 30, y: 60 })` and the others `None`, then regenerate;
  - `ui/src/ipc/schema.ts`: `bannerFocus: z.strictObject({ x: z.number().int(), y: z.number().int() }).nullable()`;
  - `ui/src/ipc/commands.ts`, `ui/src/ipc/mock.ts`;
  - `ui/src/state/queries.ts`: `useSetBannerFocus`;
  - `ui/src/art/Marquee.tsx`: a `focus` prop;
  - `ui/src/features/home/Home.tsx`;
  - `ui/src/features/home/ImageDialog.tsx` (+ test), or a new `ui/src/features/home/FocusPicker.tsx` (+ test);
  - `ui/src/styles/app.css`;
  - the en/es i18n files;
  - `DESIGN.md` (the picker paragraph).

**Interfaces:**
- Produces:
  - `Focus { x: u8, y: u8 }` (core, `Copy`, `PartialEq`);
  - `Instance.banner_focus: Option<Focus>`;
  - `AppCore::instance_banner_focus_set(&self, id: &str, focus: Option<Focus>) -> Result<Instance>`;
  - `ErrorCode::BannerFocusInvalid`;
  - UI `commands.instanceBannerFocusSet(id, focus)` and `useSetBannerFocus()`;
  - `Marquee` prop `focus?: { x: number; y: number } | null`.

- [ ] **Step 1: Write the failing tests.**
  - `packs.rs`:
    - `banner_focus_is_saved_and_reset_by_a_new_background`: set `{ x: 20, y: 80 }`, reload → equal. `instance_art_set(Generated)` → `None`.
    - `banner_focus_over_100_is_refused`: `{ x: 101, y: 0 }` → `BannerFocusInvalid`, and the file is unchanged.
    - `banner_focus_refused_while_busy`.
  - `app.rs`:
    - `update_keeps_the_banner_focus_from_disk`.
  - `instances.rs`:
    - `old_instance_json_has_no_banner_focus`.
  - UI:
    - `Marquee`: with a banner and `focus={{x:20,y:80}}`, the img style has `object-position: 20% 80%`. With no focus it keeps the CSS default (no inline style).
    - `ImageDialog`/`FocusPicker`:
      - the Position control is absent for a generated background and present with an uploaded one;
      - ArrowLeft then ArrowUp from the default sends `instance_banner_focus_set` with `{ x: 45, y: 95 }` after the debounce (fake timers);
      - "Center" sends `null`;
      - the slider-like control has `role="slider"`-free semantics: use a `role="group"` labelled "Position", and two visually hidden `input type="range"` (x and y) for keyboard and screen readers, plus the pointer surface. Assert the range inputs' labels "Left to right" / "Top to bottom".
    - The upload hints read the new copy in en.
- [ ] **Step 2:** Run the named tests. Expected: FAIL.
- [ ] **Step 3: Implement** per **Decisions → #37.1**.
  - Copy:
    - `image.position`: "Position" / "Posición";
    - `image.positionHint`: "Drag to choose which part of the picture stays in view." / "Arrastra para elegir qué parte de la imagen se ve.";
    - `image.positionX`: "Left to right" / "De izquierda a derecha";
    - `image.positionY`: "Top to bottom" / "De arriba abajo";
    - `image.center`: "Center" / "Centrar";
    - `image.uploadHintBackground`: "PNG, JPEG or WebP, up to 5 MB. Best at 1920 × 1080 or larger." / Spanish equivalent;
    - `image.uploadHintIcon`: "PNG, JPEG or WebP, up to 5 MB. Square works best." / Spanish equivalent.
  - The error `banner_focus_invalid`: en "That position is outside the picture." / es "Esa posición está fuera de la imagen."
  - Layout via `/impeccable`. The dialog must still fit at 960 × 600 in Spanish, so it may scroll internally.
  - "Center" restores the default (`null`), not `{ 50, 50 }`.
- [ ] **Step 4:** Regenerate the contract, then run the full gate. Expected: PASS.
- [ ] **Step 5:** Commit `feat: choose which part of a background picture shows (#37)`.

### Task 4: Core: read and copy another launcher's instance (#38)

**Files:**
- Create:
  - `crates/launcher-core/src/import/mod.rs` (registered in `lib.rs`): `inspect`, `copy_game_dir` and the skip rules;
  - `crates/launcher-core/src/import/detect.rs`: the four formats and the fallback.
- Modify: `crates/launcher-core/src/error.rs`: `ImportNotInstance`, `ImportInsideLauncher`, `ImportBusy`, `ImportCancelled`.

**Interfaces:**
- Produces:
  - `pub struct Detected { pub name: Option<String>, pub minecraft: Option<String>, pub loader: Option<LoaderSpec>, pub game_dir: PathBuf }`;
  - `pub async fn inspect(picked: &Path, launcher_root: &Path) -> Result<(Detected, u64 /*size_bytes*/)>`;
  - `pub async fn copy_game_dir(src: &Path, dst: &Path, skip_root_meta: bool, cancel: &CancellationToken, progress: &dyn Fn(u64 /*done*/, u64 /*total*/)) -> Result<()>` (use the cancellation type `runs.rs` already uses).

  `LoaderSpec` is the existing core type; reuse its kind enum for Fabric/Quilt/Forge/NeoForge.

- [ ] **Step 1: Write the failing tests** in `import/` with `tempfile` folders and a few bytes per file:
  - `detects_prism`:
    - `instance.cfg` with `name=My Pack`;
    - `mmc-pack.json` with the components `net.minecraft 1.20.1` and `net.fabricmc.fabric-loader 0.15.11`;
    - `.minecraft/mods/a.jar`.

    → name `My Pack`, `1.20.1`, Fabric `0.15.11`, `game_dir` ends with `.minecraft`.
  - `detects_prism_from_the_inner_folder`: picking `.minecraft` gives the same result.
  - `detects_curseforge`: `minecraftinstance.json` with `"name":"CF","gameVersion":"1.20.1","baseModLoader":{"name":"forge-47.2.0"}` → Forge `47.2.0`. `neoforge-20.4.80-beta` → NeoForge `20.4.80-beta`. `fabric-0.15.11-1.20.1` → Fabric `0.15.11`.
  - `detects_modrinth_profile`: `profile.json` `{"metadata":{"name":"MR","game_version":"1.21.1","loader":"quilt","loader_version":{"id":"0.26.0"}}}` → Quilt `0.26.0`. With `"loader":"vanilla"` → `None`.
  - `detects_atlauncher`: `instance.json` `{"id":"1.19.2","launcher":{"name":"AT","loaderVersion":{"type":"Forge","version":"43.2.0"}}}`.
  - `plain_folder_is_accepted_undetected`: only `saves/w/level.dat` → all `None`, and `game_dir` is the folder.
  - `unknown_loader_is_left_for_the_player`: Prism with `com.mumfrey.liteloader` → loader `None`, Minecraft still detected.
  - `empty_folder_is_not_an_instance` → `ImportNotInstance`.
  - `launcher_root_is_refused`: picking the root, a folder inside it, or a parent of it → `ImportInsideLauncher`. Compare canonicalized paths.
  - `copy_skips_launcher_files_and_keeps_player_data`:
    - the source has `mods/a.jar`, `mods/.index/a.pw.toml`, `saves/w/level.dat`, `XaeroWaypoints/x.txt`, `options.txt`, `assets/x`, `libraries/x`, `versions/x`, `logs/latest.log`, `.fabric/x`, `launcher_profiles.json`, and (at root, `skip_root_meta=true`) `instance.cfg`;
    - the destination holds exactly the first five;
    - `size_bytes` from `inspect` equals the bytes copied.
  - `copy_leaves_the_source_untouched`: hash every source file before and after → equal.
  - `copy_skips_symlinks` (`#[cfg(unix)]`): `saves → /some/outside/dir` isn't followed or copied.
  - `copy_honours_cancel`: a cancelled token before the second file → `ImportCancelled`.
- [ ] **Step 2:** Run `cargo test -p launcher-core import`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - Parsing uses `serde_json` with `#[serde(default)]` lenient structs. `instance.cfg` is INI: read lines `key=value` and ignore sections.
  - `copy_game_dir` walks with `tokio::fs::read_dir`, uses `symlink_metadata` to skip links, and copies each file through `tokio::fs::copy`. It checks `cancel.is_cancelled()` between files and reports cumulative bytes.
  - Errors:
    - `import_not_instance`: en "This folder doesn't look like a Minecraft instance." / fix "Pick the instance's own folder: the one with mods, saves or config inside." es "Esta carpeta no parece una instancia de Minecraft." / "Elige la carpeta de la instancia: la que tiene mods, saves o config adentro."
    - `import_inside_launcher`: en "That folder belongs to Perland Launcher itself." / fix "Pick a folder from your other launcher." es "Esa carpeta es del propio Perland Launcher." / "Elige una carpeta de tu otro launcher."
    - `import_busy`: en "Another import is still running." / fix "Wait for it to finish, or cancel it." es "Otra importación sigue en curso." / "Espera a que termine o cancélala."
    - `import_cancelled`: en "The import was cancelled." es "Se canceló la importación." (no fix)
  - Categories: `Disk` for the first three, `Disk` for cancelled.
- [ ] **Step 4:** Run the full gate. Expected: PASS.
- [ ] **Step 5:** Commit `feat(core): read and copy an instance from another launcher (#38)`.

### Task 5: App and UI: "From another launcher" (#38)

**Files:**
- Create: `crates/launcher-app/src/import.rs` (registered in `lib.rs`), `ui/src/features/new-instance/ImportForm.tsx` (+ `ImportForm.test.tsx`), `ui/src/state/importProgress.ts`
- Modify:
  - `crates/launcher-app/src/dto.rs`: `ImportPreview`, `ImportRequest`;
  - `crates/launcher-app/src/app.rs`: the import slot (a `Mutex<Option<CancellationToken>>`);
  - `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`;
  - `crates/launcher-app/tests/contract.rs`: `import_preview` and `import_request` fixtures, then regenerate;
  - `ui/src/ipc/schema.ts`, `ui/src/ipc/commands.ts`, `ui/src/ipc/mock.ts` (a fake preview, and a fake copy that emits progress over about 2 s);
  - `ui/src/state/queries.ts`, `ui/src/state/handleEvent.ts`: route `import_instance`;
  - `ui/src/features/new-instance/NewInstance.tsx` (+ test), `VersionFields.tsx` if it needs an initial value;
  - the en/es i18n files;
  - `PRODUCT.md`: the capabilities list;
  - `DESIGN.md`: the New instance paragraph.

**Interfaces:**
- Consumes: Task 4's `inspect` and `copy_game_dir`; `InstanceStore::create`; the settings default memory as in `instance_create` (`app.rs:319-342`).
- Produces:
  - `ImportPreview { path: PathBuf, name: Option<String>, minecraft: Option<String>, loader: Option<LoaderSpec>, size_bytes: u64 }` (camelCase);
  - `ImportRequest { path: PathBuf, name: String, minecraft: String, loader: Option<LoaderSpec> }` (camelCase);
  - `AppCore::import_inspect(&self, path: PathBuf) -> Result<ImportPreview>`;
  - `AppCore::import_instance(&self, request: ImportRequest) -> Result<Instance>`;
  - `AppCore::import_cancel(&self)`;
  - the Tauri commands `import_inspect`, `import_instance` and `import_cancel`;
  - UI `commands.importInspect(path)`, `commands.importInstance(request)` and `commands.importCancel()`.

- [ ] **Step 1: Write the failing tests.**
  - `import.rs`:
    - `import_creates_a_plain_instance_with_the_copied_files`: a Prism fixture → the instance has the given name and game, default memory, and `minecraft/mods/a.jar` and `minecraft/XaeroWaypoints/x.txt` exist. `mods_list` shows `a.jar`. Progress events with task key `import_instance` were emitted, and the last one has `done == total`.
    - `cancelled_import_leaves_nothing`: cancel from the progress callback → `ImportCancelled`. The listing is unchanged, and `<root>/import-staging` is empty or gone.
    - `failed_import_leaves_nothing`: make the destination unwritable by pre-creating `import-staging` as a file → error, and no instance.
    - `second_import_while_one_runs_is_refused` → `ImportBusy`.
    - `name_is_validated`: an empty name → the existing invalid-name code, and nothing is copied.
  - `ImportForm.test.tsx` (mock IPC; mock `open` from `@tauri-apps/plugin-dialog` as the other tests do):
    - Choose folder → `import_inspect` is called. The form shows the name, Minecraft version and loader from the preview, and the size line "We'll copy 1.2 GB…".
    - Import → `import_instance` with the edited name. Progress events show the meter. On resolve, the route goes home to the new id.
    - Cancel during progress → `import_cancel`.
    - An `import_not_instance` error from inspect shows the translated ErrorCard, and you can choose again.
  - `NewInstance.test.tsx`: the fourth mode "From another launcher" renders `ImportForm`.
- [ ] **Step 2:** Run the named tests. Expected: FAIL.
- [ ] **Step 3: Implement** per **Decisions → #38**.
  - Copy:
    - `newInstance.modeImport`: "From another launcher" / "De otro launcher";
    - `import.choose`: "Choose folder…" / "Elegir carpeta…";
    - `import.hint`: "In your other launcher, open the instance's folder (usually right-click → Open folder) and pick it here." / "En tu otro launcher, abre la carpeta de la instancia (normalmente clic derecho → Abrir carpeta) y elígela aquí.";
    - `import.size`: "We'll copy {{size}}: your mods, worlds, settings and map data. The original folder isn't changed." / "Vamos a copiar {{size}}: tus mods, mundos, ajustes y datos de mapas. La carpeta original no se toca.";
    - `import.submit`: "Import" / "Importar";
    - `import.copying`: "Copying… {{done}} of {{total}}" / "Copiando… {{done}} de {{total}}";
    - `import.pickTitle`: "Pick the instance's folder" / "Elige la carpeta de la instancia".
  - Sizes go through the existing byte formatter in `ui/src/ui/format.ts`.
  - Only "Import" is lit; "Choose folder…" and Cancel are raised.
  - Styling via `/impeccable`.
  - `import_instance` validates the name **before** copying.
- [ ] **Step 4:** Regenerate the contract, then run the full gate. Expected: PASS.
- [ ] **Step 5:** Commit `feat: import a whole instance from another launcher (#38)`.

### Task 6: Finish

- [ ] Whole-branch review on the most capable model.
- [ ] Mock screenshots via Playwright against `pnpm --filter ui dev:mock`, saved to the scratchpad:
  - Home with playtime;
  - Accounts total;
  - the title bar cog;
  - the rail with round icons, compact and wide;
  - Settings;
  - the picker with Position;
  - the import form, before, during and after.
- [ ] Draft PR to `main` with:
  - `Fixes #37` is **not** used, because #37.2 is open to the owner. Use `Refs #37`, `Fixes #38` and `Refs #32` (achievements and medals are deferred) and `Refs #36`;
  - the list of our calls for owner review;
  - a Windows smoke-test list:
    - import from a real Prism instance and a real CurseForge instance, then play, with Xaero waypoints visible in game;
    - cancel a large import;
    - check that playtime increments after a session;
    - the cog renders crisply at 125% scaling.
- [ ] Reporter updates follow `reporter-updates`. Draft them, and ask the owner before posting.
- [ ] An execution-notes section in this plan.
