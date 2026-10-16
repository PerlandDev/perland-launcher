# Closed-beta feedback, round 2 (#22, #23, #24, #28) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address the second round of reports from the closed-beta tester. The four changes:
- custom Java arguments per instance (#23);
- an instance icon kept apart from the big background and shown in its own frame (#22);
- a generated icon you can recognise (#24);
- Browse that a new player can find (#28).

**Architecture:** One PR off `main` on branch `claude/wizardly-curie-txsrak`.
- **Core:**
  - `JavaSettings.args`, plus a small argument tokenizer and validator in `launch/jvm_args.rs`;
  - `Instance.icon` (`Option<IconMode>`) next to the existing background `art`.
- **App:**
  - `instance_icon_set`;
  - `instance_art` resolves the icon and the background separately.
- **UI:**
  - the Java arguments field;
  - a generated monogram icon;
  - the icon plate on Home;
  - a two-section "Change image…" dialog;
  - Browse entry points on first run, the empty home and the vanilla Mods screen.

**Tech Stack:** Rust (tokio, serde), Tauri 2.12, React 19, TanStack Query, zod, Vitest, Testing Library, i18next.

**Spec:** this document. The design was owner-delegated in chat on 2026-10-16: the owner answered #28 ("reply plus the discoverability change") and handed every other call to the implementer to decide autonomously. Every decision below that the owner didn't make is marked **(our call)**, for the owner to review on the PR. Prior art: `docs/superpowers/plans/2026-10-14-beta-feedback.md` (round 1: #8's picker and the rail), `docs/superpowers/specs/2026-10-11-browse-design.md`.

## Decisions

### #28: install mods or community modpacks (owner: reply plus discoverability)
- **Browse already does this.** It shipped in 0.2.0-beta (PR #16, merged 2026-10-12, an ancestor of the `v0.2.0-beta.*` tags).
- **Why the tester missed it:**
  - first run creates a vanilla instance;
  - a vanilla Mods screen hides "Find mods";
  - the rail's Browse key is only a magnifier until the rail is widened;
  - "Browse modpacks" sits only in the New instance header.
- **Add three entry points (our call on where):**
  1. **First run, step 3 ("Your first instance"):** a secondary "Browse modpacks" button under "Let's play". It finishes first run (`firstRunDone: true`, language saved) without creating an instance, then opens `{ name: 'browse', kind: 'modpack' }`.
  2. **Empty home:** a secondary "Browse modpacks" button beside "Create an instance".
  3. **Vanilla Mods screen:** the existing `mods.vanillaNote` plate gains a "Browse modpacks" text button. Modpacks come with their loader, and vanilla can't load mods, so it points to modpacks, not "Find mods".
- **No other changes:** the rail key stays as it is.
- **PRODUCT.md:50** still says Browse is "Not built". Fix that line.

### #23: custom Java arguments (all our call)
- **Storage:** `instance.json` → `java.args: String`, with serde default `""`.
  - It is stored as the player typed it, so the field shows exactly what they wrote.
  - It is parsed when saved (validation) and again at launch, since a hand-edited file must not launch with bad input.
- **Parsing (`launch::jvm_args::parse`):**
  - Arguments are split on whitespace, newlines included.
  - `"…"` and `'…'` group text that has spaces. The quotes are removed and can sit mid-token: `-Dname="My Pack"` gives `-Dname=My Pack`.
  - A backslash is always literal, because Windows paths need it.
  - To put a `"` inside an argument, wrap it in `'…'`, and the other way round.
  - An unclosed quote gives `jvm_args_unclosed_quote`.
- **Rules, applied to each argument after parsing:**
  - **It must start with `-`.** Otherwise: `jvm_arg_not_option` with `{arg}`. This blocks a bare word, which Java would take as the main class, and an `@argfile`.
  - **Memory:** `-Xmx…` gives `jvm_args_memory`, because memory is the slider. `-Xms…` is allowed.
  - **Refused with `jvm_arg_not_allowed` and `{arg}`:**
    - **Class and module path, main-class selection:** `-cp`, `-classpath`, `--class-path`, `--class-path=…`, `-p`, `--module-path`, `--module-path=…`, `-jar`, `-m`, `--module`, `--module=…`. The launcher sets these itself.
    - **Exit before launching:** `-version`, `--version`, `-help`, `--help`, `-h`, `-?`, `-X`, `--help-extra`, `--dry-run`, `--list-modules`, `-d`, `--describe-module`, `--validate-modules`.
    - Matching is exact, except the `…=` forms and `-Xmx`, which match by prefix.
  - **Length:** over 8192 characters in total (the raw string) gives `jvm_args_too_long` with `{max}`.
- **Order in `build_command`:**
  1. `-Xmx{memory}M`
  2. **the player's arguments**
  3. the version JSON's JVM args (or the legacy `-Djava.library.path`/`-cp`)
  4. log4j
  5. main class
  6. game args

  Java keeps the last value of a repeated `-D`, so the launcher's own properties (library path, brand, log4j config) always win over a player's. The player's args sit before the main class, so on Windows they go into the argfile with the rest of the JVM args, already quoted correctly.
- **When changes apply:** `JavaSettings` already makes Java changes wait until the game stops (`AppCore::update`). Nothing new is needed.
- **Error codes (category `java`):** `jvm_args_unclosed_quote`, `jvm_arg_not_option`, `jvm_arg_not_allowed`, `jvm_args_memory`, `jvm_args_too_long`. Each needs en/es `message` and `fix`.
  - Params carry only the player's own text or a number, never English (PR #25's rule).
- **Rule changes from the final review:**
  - Module options that take a value (`--add-opens`, `--add-exports`, `--add-reads`, `--add-modules`, `--enable-native-access`, `--patch-module`, `--limit-modules`) consume a following word that doesn't start with `-`. That word skips the dash rule. A dashed word is still checked as an option, and a module option with nothing after it passes through for Java to report.
  - `-XX:MaxHeapSize=`, `-XX:MaxRAM=` and `-XX:MaxRAMPercentage=` are refused with `jvm_args_memory`, like `-Xmx`, because they override the slider. `-Xms` and `-XX:InitialHeapSize=` stay allowed.
- **Deferred:** a global default in Settings. The report asks per instance, and it would be the first copy-on-create text setting.

### #24: recognisable logos (our call)
- **Real icons already arrive:**
  - Modrinth modpacks from Browse (`icon_url`);
  - packwiz `launcher.toml` `[art] icon`;
  - server invites' `/icon`.
- **The "lines":** the generated `PlotIcon` is a one-hue staircase over dirt, with only 4 hues.
- **New generated icon, `InstanceIcon`:** a monogram tile.
  - 1–2 initials of the instance **name**: the first grapheme of the first two words, split on whitespace, `-` and `_`. A one-word name gives one letter. `toLocaleUpperCase()`.
  - Bungee on a plate in the instance's hue (same `hueIndex(artSeed)` as today), with the terraced-plot motif kept as a quiet base.
  - If the name has no letter or digit, it falls back to today's `PlotIcon`.
  - The exact look goes through `/impeccable`.
- **Where it replaces `PlotIcon`:**
  - the rail fallback;
  - the picker's "Generated" icon tile;
  - the Home icon plate (see #22).
- **Unchanged:** Browse result rows and server cards keep `PlotIcon`.
- **Deferred:** an icon for local `.mrpack` imports. The format has no icon, and finding one needs a Modrinth lookup by file hash.

### #22: icon separate from the background (our call)
- **Model:** `Instance.icon: Option<IconMode>`.
  - `IconMode` is `Generated | Custom | Auto` (lowercase; `Auto` is the `serde(other)` default, like `ArtMode`).
  - `None` means "never chosen". `Instance::icon_mode()` resolves it: `None` → `Custom` if `art == Custom` (a round-1 upload keeps filling both places), otherwise `Auto`.
  - `art` and `art_seed` keep meaning the background, unchanged.
- **Files:**
  - the custom background stays at `.perland/custom/image.{png,jpg,webp}`;
  - the custom icon goes to `.perland/custom/icon.{png,jpg,webp}`;
  - a legacy instance (`icon == None && art == Custom`) uses `image.*` as its icon.
- **`instance_art` resolution:**
  - **icon:**
    - `Custom` → `custom/icon.*`, or for a legacy instance `custom/image.*`;
    - `Auto` → `art/icon.*`;
    - `Generated` → `None`.
  - **banner:**
    - `art == Custom` → `custom/image.*`;
    - `Auto` → `art/banner.*`;
    - `Generated` → `None`.
  - The UI no longer copies the icon into the banner.
- **`instance_icon_set(id, IconChoice)`:**
  - The choices are `Generated` (sets `icon = Some(Generated)`), `Pack` (sets `Some(Auto)`; `no_pack` unless the instance is a pack) and `Upload { path }` (sets `Some(Custom)` and writes `icon.<ext>`).
  - Busy and installing refusals, upload checks and atomic writes match `instance_art_set`.
  - Generated and Pack delete `custom/icon.*`.
- **`instance_art_set` (background) changes:**
  - It no longer removes the whole `custom/` dir, only `custom/image.*`.
  - Before it changes a legacy instance, it **pins** the icon: it copies `image.<ext>` to `icon.<ext>` and sets `icon = Some(Custom)`. A new background then never takes the old icon with it.
- **Home:**
  - When the instance has a **real** icon image (`art.data.icon`), a framed **icon plate** sits left of the name: about 4.5rem, a laminate plate on the dirt step, pixel corners, the image inside with `pixelUpscale`.
  - Generated instances show no plate, because the monogram would only repeat the name.
- **"Change image…" dialog:** two labelled sections in one dialog.
  - **"Icon":** Generated (monogram), Pack icon (packs only), Upload your own….
  - **"Background":** the 6 landscapes, Pack art (packs only), Upload your own….
  - Choosing one applies at once, as today.
  - The menu label stays "Change image…".

## Execution notes (2026-10-16)

- **Shipped:** PerlandDev/perland-launcher#30, merged as `dc5e001`. It closed #22, #23, #24 and #28. The calls made on the owner's behalf are listed in that PR's body.
- **Changes during execution:**
  - Any background change pins the current icon, not only a legacy one. Without this, a never-chosen icon follows a new background upload.
  - The picker is 46rem with its sections side by side.
  - The monogram plate is tinted laminate, not the pure hue.
- **Reporters:** start-of-work updates went out on all four issues. Nothing more is due until a build carrying this ships. Then a short note on "Cambiar imagen…" (Ícono and Fondo) and the Java arguments field would help the tester.
- **Deferred:**
  - a global default for Java arguments;
  - an icon for local `.mrpack` imports, which needs a Modrinth lookup by file hash;
  - the mock's `instance_update` only refuses `-Xmx`, not the other memory flags or the module-option rule;
  - a legacy round-1 upload is sent as both icon and banner until the player changes one;
  - a hand-edited bad `args` is caught only after downloads, and it also refuses an optional-mod toggle;
  - in Spanish with the wide rail at 960 px, the update plate's "Actualizar" button wraps onto its own line. This predates this branch.

## Global Constraints

- `crates/launcher-core` and `crates/launcher-app` never depend on Tauri. `src-tauri` is wiring only and has no tests.
- TDD: Rust unit tests in `#[cfg(test)]`; UI uses Vitest + Testing Library.
- Every UI string goes in `ui/src/i18n/en.json` and `ui/src/i18n/es.json` (neutral Latin American Spanish, informal "tú"). Every new `ErrorCode` needs `errors.<code>.message` (and here `.fix`) in both.
- `ui/src/ipc/contract/*.json` is regenerated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. It is never hand-edited. `ui/src/ipc/schema.ts` uses `z.strictObject`, so a new Rust field needs its zod field in the same task.
- UI styling goes through `/impeccable`, against `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`:
  - minimum window 960 × 600;
  - Spanish runs about 30% longer;
  - at rest, only one button per view is lit;
  - hard offset steps, never blurred shadows.
- No Modrinth or Mojang branding or assets: show a project's own icon only.
- Tokens never reach logs. Never run fuzz, stress or load tests locally; keep test inputs tiny.
- The commit trailer is exactly `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Subagent prompts repeat it verbatim.
- Run the full gate before every task-ending commit:
  - `pnpm --filter ui build`
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `pnpm -r lint`
  - `pnpm -r typecheck`
  - `pnpm -r test`
- PR #25 (hardening) is open and touches `error.rs`, the i18n files, `instances.rs` and `packs.rs`. Keep edits there additive and small.

## Review Focus

1. **Windows paths in arguments:** `-Djava.io.tmpdir="C:\Users\José\Mi Carpeta"` must reach Java as one argument with the backslashes intact, both through the argfile and on the direct command line. Task 1 tests the parse, and Task 1 tests the argfile.
2. **An old round-1 upload:** an instance saved with `art: "custom"` and no `icon` key must still show the upload as both icon and background. Choosing a landscape background must then keep that upload as the icon. Task 3 tests both.
3. **Hand-edited `instance.json`:** a stored `args` with an unclosed quote or `-cp` must refuse Play with the same translated error, not launch with a broken command. Task 1 tests `play` command building.
4. **Stale UI copy:** `instance_update` from an old UI copy must not clear `icon`. `AppCore::update` keeps the art fields from disk, so `icon` joins them. Task 3 tests it.
5. **Accessibility of the monogram:** the rail cabinet's accessible name stays the instance name, and the monogram is `aria-hidden`. Task 4 tests it.

---

### Task 1: Core: Java arguments (#23)

**Files:**
- Create: `crates/launcher-core/src/launch/jvm_args.rs` (registered in `launch/mod.rs`)
- Modify:
  - `crates/launcher-core/src/instances.rs`: add `JavaSettings.args`, and call `jvm_args::parse` from `validate_java`;
  - `crates/launcher-core/src/launch/mod.rs`: `LaunchSpec.jvm_args: Vec<String>`, and `build_command` order;
  - `crates/launcher-core/src/play.rs`: fill it from `parse(&instance.java.args)?`;
  - `crates/launcher-core/src/error.rs`: the 5 codes in the `// java` group;
  - `ui/src/i18n/en.json`, `ui/src/i18n/es.json`: `errors.<code>.message` and `.fix`;
  - `crates/launcher-app/tests/contract.rs`: the `JavaSettings` literals. Give `vanilla_instance` `args: "-XX:+UseG1GC -Dperland.note=\"a b\""`, keep the others empty, then regenerate the contract;
  - `ui/src/ipc/schema.ts`: `javaSettings` gains `args: z.string()`;
  - `ui/src/ipc/mock.ts`: the `java` literals gain `args: ''`;
  - every other Rust `JavaSettings { .. }` literal.

**Interfaces:**
- Produces: `pub fn parse(raw: &str) -> Result<Vec<String>>` (the `launcher_core::launch::jvm_args::parse`), the `JavaSettings.args: String` (camelCase `args`), the `LaunchSpec.jvm_args: Vec<String>`, the `ErrorCode::{JvmArgsUnclosedQuote, JvmArgNotOption, JvmArgNotAllowed, JvmArgsMemory, JvmArgsTooLong}` and `pub const MAX_JVM_ARGS_CHARS: usize = 8192`.

- [ ] **Step 1: Write the failing tests** in `jvm_args.rs`:
  - `splits_on_whitespace_and_newlines`: `"-XX:+UseG1GC\n  -Dx=1"` → `["-XX:+UseG1GC", "-Dx=1"]`; `""` and `"  \n"` → `[]`.
  - `quotes_group_and_are_removed`: `-Dname="My Pack" -Dq='say "hi"'` → `["-Dname=My Pack", "-Dq=say \"hi\""]`.
  - `backslashes_are_literal`: `-Djava.io.tmpdir="C:\Users\José\Mi Carpeta"` → `["-Djava.io.tmpdir=C:\Users\José\Mi Carpeta"]`.
  - `unclosed_quote_is_refused`: `-Dx="a` → code `JvmArgsUnclosedQuote`.
  - `every_arg_is_an_option`: `foo`, `@args.txt` → `JvmArgNotOption` with param `arg` = the token.
  - `launcher_owned_args_are_refused`: each of `-cp`, `-classpath`, `--class-path=x`, `-p`, `--module-path=x`, `-jar`, `-m`, `--module=x`, `-version`, `--help`, `-?`, `-X`, `--dry-run` → `JvmArgNotAllowed` with `arg`. `-Xss4M`, `-Xms2G`, `-XX:+UseZGC` and `--add-opens=java.base/java.lang=ALL-UNNAMED` are fine.
  - `memory_goes_through_the_slider`: `-Xmx8G` → `JvmArgsMemory`.
  - `too_long_is_refused`: `"-Da".repeat(…)` over 8192 chars → `JvmArgsTooLong` with param `max` = `"8192"`.

  In `instances.rs`:
  - `update_refuses_bad_java_args`: update with `args: "-cp x"` → `JvmArgNotAllowed`, file unchanged.
  - `old_instance_json_loads_with_empty_args`: no `args` key → `""`.

  In `launch/mod.rs`:
  - `player_jvm_args_follow_memory_and_precede_the_version_args`: with `jvm_args = ["-XX:+UseG1GC", "-Dperland.x=1"]` on the 1.20.1 fixture, `args[0] == "-Xmx4096M"`, `args[1..3]` are the player's, and the brand flag and `-cp` come after them.
  - `player_jvm_args_go_into_the_argfile`: a long classpath with `jvm_args = ["-Ddir=C:\\A B\\c"]`. The `with_argfile` text contains `"-Ddir=C:\\\\A B\\\\c"`, and the command line holds only `@…`, the main class and the game args.

  In `play.rs`:
  - `stored_bad_args_refuse_to_build_a_command`: a hand-written `args: "-Dx=\"a"` → `command()` fails with `JvmArgsUnclosedQuote`.
- [ ] **Step 2:** Run `cargo test -p launcher-core jvm_args` and the named tests. Expected: they FAIL (missing module and fields).
- [ ] **Step 3: Implement.**
  - `parse` is a single pass over `chars()` with an `Option<char>` quote state and an `in_token` flag. An empty quoted string (`""`) still makes a token.
  - Check the length first (`raw.chars().count()`), then the rules, in the order the tests list them.
  - Errors: `LauncherError::java(code, "<plain English message>").with_param(...)`.
  - Messages:
    - `jvm_args_unclosed_quote`: en "A quote in the Java arguments isn't closed." / fix "Add the closing quote, or remove the opening one." es "Una comilla de los argumentos de Java no está cerrada." / "Agrega la comilla que cierra, o quita la que abre."
    - `jvm_arg_not_option`: en "“{{arg}}” isn't a Java option. Each argument starts with a dash." / fix "Remove it, or add the dash it's missing." es "“{{arg}}” no es una opción de Java. Cada argumento empieza con un guion." / "Quítalo, o agrégale el guion que le falta."
    - `jvm_arg_not_allowed`: en "“{{arg}}” can't be used here. The launcher sets the class path, the main class and how Java starts." / fix "Remove it from the Java arguments." es "“{{arg}}” no se puede usar aquí. El launcher define el class path, la clase principal y cómo arranca Java." / "Quítalo de los argumentos de Java."
    - `jvm_args_memory`: en "Memory is set with the slider, not in the Java arguments." / fix "Remove -Xmx and move the memory slider instead." es "La memoria se elige con el control deslizante, no en los argumentos de Java." / "Quita -Xmx y mueve el control de memoria."
    - `jvm_args_too_long`: en "The Java arguments are too long (over {{max}} characters)." / fix "Remove the arguments you don't need." es "Los argumentos de Java son demasiado largos (más de {{max}} caracteres)." / "Quita los argumentos que no necesitas."
- [ ] **Step 4:** Regenerate with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, then run the full gate. Expected: all PASS. `i18n.test.ts` covers the new codes.
- [ ] **Step 5:** Commit `feat(core): custom Java arguments per instance (#23)`.

### Task 2: UI: the Java arguments field (#23)

**Files:**
- Modify:
  - `ui/src/features/instance-settings/InstanceSettings.tsx` and its test;
  - `ui/src/ipc/mock.ts`: `instance_update` refuses `-Xmx` with `jvm_args_memory`, so the error shows in mock mode;
  - the en/es i18n files.

**Interfaces:**
- Consumes: Task 1's `java.args`, and the error codes starting `jvm_`.

- [ ] **Step 1: Write the failing tests** in `InstanceSettings.test.tsx`:
  - `saves the Java arguments as typed`: type `-XX:+UseG1GC -Dx="a b"`, Save → `saved.instance.java` equals `{ memoryMb, path, args: '-XX:+UseG1GC -Dx="a b"' }`. Update the existing exact `toEqual`.
  - `shows a refused argument under the field`: the mock refuses `-Xmx4G` → the field's error text is `errors.jvm_args_memory.message` translated, and the field has `aria-invalid`.
- [ ] **Step 2:** Run `pnpm --filter ui test -- InstanceSettings`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - Form state `javaArgs`, initialised from `instance.java.args`. `save()` sends `args: javaArgs`.
  - A `<Field>` labelled `instanceSettings.javaArgs` ("Java arguments" / "Argumentos de Java") with hint `instanceSettings.javaArgsHint` ("Optional, for advanced players. Separate them with spaces and put quotes around a value with spaces. Memory is set above." / "Opcional, para jugadores avanzados. Sepáralos con espacios y pon comillas a un valor que tenga espacios. La memoria se elige arriba.").
  - The input is a `textarea` with 2 rows, `font-mono`, `spellCheck={false}`, `autoComplete="off"`.
  - It goes after the Java fieldset.
  - A save error whose code starts with `jvm_` goes to the field's `error` (translated through `describeError`) instead of the general error card.
  - Styling goes through `/impeccable`.
- [ ] **Step 4:** Run the full gate. Expected: PASS.
- [ ] **Step 5:** Commit `feat(ui): Java arguments in instance settings (#23)`.

### Task 3: App: the icon separate from the background (#22)

**Files:**
- Modify:
  - `crates/launcher-core/src/instances.rs`: `IconMode`, `Instance.icon`, `Instance::icon_mode()`, and `create` sets `icon: None`;
  - `crates/launcher-app/src/dto.rs`: `IconChoice`;
  - `crates/launcher-app/src/packs.rs`: `instance_art` resolution, `instance_icon_set`, and changes to `instance_art_set`;
  - `crates/launcher-app/src/app.rs`: `update` keeps `icon` from disk, like `art`;
  - `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`: the `instance_icon_set` command;
  - `crates/launcher-app/tests/contract.rs`: samples, plus an `icon_choices` fixture like `art_choices`, then regenerate;
  - `ui/src/ipc/schema.ts`: `icon: z.enum(['auto','generated','custom']).nullable()` and `iconChoice`;
  - `ui/src/ipc/commands.ts`: `instanceIconSet`;
  - `ui/src/state/queries.ts`: `useSetInstanceIcon`, mirroring `useSetInstanceArt`;
  - `ui/src/ipc/mock.ts`: `instance_icon_set`, and `instance_art` returns the banner for custom;
  - `ui/src/features/home/Home.tsx`: `banner = art.data?.banner ?? null`.

**Interfaces:**
- Produces:
  - `IconMode` (core);
  - `Instance.icon: Option<IconMode>` (camelCase `icon`, `null` when unset);
  - `Instance::icon_mode(&self) -> IconMode`;
  - `IconChoice` = `#[serde(tag = "kind", rename_all = "lowercase")] enum { Generated, Pack, Upload { path: PathBuf } }`;
  - `AppCore::instance_icon_set(&self, id: &str, choice: IconChoice) -> Result<Instance>`;
  - UI `commands.instanceIconSet(id, choice)`;
  - `useSetInstanceIcon(id)`.

- [ ] **Step 1: Write the failing tests** in `packs.rs`, with tiny PNG bytes as in the round-1 tests:
  - `icon_and_background_uploads_are_separate`: upload A as the icon and B as the background. `instance_art` gives icon A and banner B. Choosing a landscape background keeps icon A.
  - `legacy_upload_fills_both_until_changed`: write `art: "custom"` with no `icon` key, plus `custom/image.png`. `instance_art` gives that image as both icon and banner. Then `instance_art_set(Generated)` → banner `None`, the icon is still the image, `custom/icon.png` exists, and `icon == Some(Custom)`.
  - `pack_icon_choice`:
    - `IconChoice::Pack` on a non-pack → `no_pack`;
    - on a pack it sets `Some(Auto)` and returns `art/icon.*`;
    - `IconChoice::Generated` → icon `None`, even with pack art present, and `custom/icon.*` is removed.
  - `icon_uploads_are_checked`: an exe named `.png` → `art_file_invalid`, and `icon` is unchanged.
  - `icon_set_refused_while_busy`.

  In `app.rs`:
  - `update_keeps_the_icon_from_disk`.

  In `instances.rs`:
  - `icon_mode_resolves_legacy`: `None` + `Custom` → `Custom`; `None` + `Generated` → `Auto`.
- [ ] **Step 2:** Run `cargo test -p launcher-app icon` and `cargo test -p launcher-core icon_mode`. Expected: FAIL.
- [ ] **Step 3: Implement** per **Decisions → #22**. Share the upload and write path between `instance_art_set` and `instance_icon_set` through a helper `write_custom(dir, stem, bytes, ext)` that removes the other extensions of the same stem.
- [ ] **Step 4:** Regenerate the contract. The UI tests that build instances gain `icon: null`. Run the full gate. Expected: PASS.
- [ ] **Step 5:** Commit `feat(app): keep an instance's icon apart from its background (#22)`.

### Task 4: UI: a generated monogram icon (#24)

**Files:**
- Create: `ui/src/art/monogram.ts` (+ `monogram.test.ts`), `ui/src/art/InstanceIcon.tsx` (+ `InstanceIcon.test.tsx`)
- Modify: `ui/src/features/rail/Rail.tsx` (the fallback), `ui/src/styles/app.css` if needed, and `DESIGN.md` (the Rail paragraph: generated monogram icon)

**Interfaces:**
- Produces:
  - `initials(name: string): string` (0–2 graphemes, upper-cased; `''` when there's no letter or digit);
  - `InstanceIcon({ name, seed, size }: { name: string; seed: string; size?: number })`, which renders `aria-hidden` SVG and falls back to `PlotIcon` when `initials` is `''`.

- [ ] **Step 1: Write the failing tests:**
  - `initials`: `"Asalith Fields"` → `"AF"`; `"creative"` → `"C"`; `"my-cool_pack"` → `"MC"`; `"  ñandú  "` → `"Ñ"`; `"1.21 Survival"` → `"1S"`; `"!!!"` → `""`; `"👍 pack"` → `"P"` (non-letter graphemes are skipped).
  - `InstanceIcon`:
    - it renders the initials text, and its hue fill equals `PLOT_HUES[hueIndex(seed)]`;
    - it is `aria-hidden`;
    - `"!!!"` renders the plot cells.
  - Rail: a cabinet without art shows the `InstanceIcon` initials, and its accessible name is still the instance name (extend the existing Rail test file).
- [ ] **Step 2:** Run `pnpm --filter ui test -- monogram InstanceIcon Rail`. Expected: FAIL.
- [ ] **Step 3: Implement.** Graphemes via `Intl.Segmenter` (granularity `grapheme`); a grapheme counts if it matches `/[\p{L}\p{N}]/u`. Visual design via `/impeccable`: a Bungee monogram on the hue plate with the plot base. It must stay legible at 40px with two letters.
- [ ] **Step 4:** Run the full gate. Expected: PASS.
- [ ] **Step 5:** Commit `feat(ui): recognisable generated instance icons (#24)`.

### Task 5: UI: the Home icon plate and the two-part picker (#22)

**Files:**
- Modify:
  - `ui/src/features/home/Home.tsx` (+ `Home.test.tsx`);
  - `ui/src/features/home/ImageDialog.tsx` (+ `ImageDialog.test.tsx`);
  - `ui/src/styles/app.css`;
  - `ui/src/i18n/en.json`, `ui/src/i18n/es.json`;
  - `DESIGN.md`: the Marquee and picker paragraphs.

**Interfaces:**
- Consumes: Task 3's `useSetInstanceIcon` and `instance.icon`; Task 4's `InstanceIcon`.

- [ ] **Step 1: Write the failing tests:**
  - Home: with `instanceArt` returning an icon, an `img` with the plate class sits before the `h1` in the marquee overlay. Without one, there is no plate.
  - ImageDialog:
    - two groups, "Icon" and "Background" (`role="radiogroup"`, labelled);
    - picking "Generated" in Icon sends `instance_icon_set` `{ kind: 'generated' }`;
    - the third landscape still sends `instance_art_set` `{ kind: 'generated', seed: '<base>~3' }`;
    - "Pack icon" appears only for packs;
    - the Icon group's current choice comes from `icon ?? (art === 'custom' ? 'custom' : 'auto')`.
- [ ] **Step 2:** Run `pnpm --filter ui test -- Home ImageDialog`. Expected: FAIL.
- [ ] **Step 3: Implement** via `/impeccable` (one lit button: Play stays the only amber).
  - New strings: `imageDialog.iconSection` "Icon" / "Ícono", `imageDialog.backgroundSection` "Background" / "Fondo", `imageDialog.generatedIcon` "Initials" / "Iniciales", `imageDialog.packIcon` "Pack icon" / "Ícono del pack".
  - Reuse the existing upload and pack labels where they fit.
- [ ] **Step 4:** Run the full gate. Expected: PASS.
- [ ] **Step 5:** Commit `feat(ui): instance icon plate and separate icon/background picker (#22)`.

### Task 6: UI: Browse a new player can find (#28)

**Files:**
- Modify:
  - `ui/src/features/first-run/FirstRun.tsx` (+ test);
  - `ui/src/features/home/EmptyState.tsx` (+ a test in `Home.test.tsx`);
  - `ui/src/features/mods/Mods.tsx` (+ its test);
  - the en/es i18n files;
  - `PRODUCT.md:50`.

**Interfaces:**
- Consumes: the existing route `{ name: 'browse', kind: 'modpack' }` and the existing string `browse.browseModpacks`.

- [ ] **Step 1: Write the failing tests:**
  - FirstRun: on step 3, "Browse modpacks" saves settings with `firstRunDone: true` and the chosen language, creates **no** instance, and the route becomes browse/modpack.
  - EmptyState: a "Browse modpacks" button navigates to browse/modpack.
  - Mods on a vanilla instance: the vanilla note holds a "Browse modpacks" button that navigates to browse/modpack. It is absent on a Fabric instance.
- [ ] **Step 2:** Run `pnpm --filter ui test -- FirstRun Home Mods`. Expected: FAIL.
- [ ] **Step 3: Implement.** The buttons are secondary (not lit), with the Search icon, via `/impeccable`. If a new line is needed, add `firstRun.browseInstead` "Or start from a community modpack" / "O empieza con un modpack de la comunidad".
- [ ] **Step 4:** Run the full gate. Expected: PASS.
- [ ] **Step 5:** Commit `feat(ui): make Browse easy to find for new players (#28)`.

### Task 7: Finish

- [ ] Whole-branch review on the most capable model.
- [ ] Mock screenshots via Playwright against `pnpm --filter ui dev:mock`:
  - Home with a pack icon plate;
  - the picker;
  - the rail with monograms, compact and wide;
  - instance settings with an argument error;
  - first run, step 3;
  - the empty home.

  Save them to the scratchpad and show them to the owner.
- [ ] Draft PR to `main`:
  - `Fixes #22`, `Fixes #23`, `Fixes #24`, `Fixes #28`;
  - our calls listed for owner review;
  - the Windows smoke-test list:
    - launch with `-Dx="C:\A B"` and a long Forge classpath (argfile);
    - an `-Xmx` refusal;
    - a round-1 uploaded image survives;
    - icon and background uploads are separate;
    - first-run Browse;
    - an `/impeccable` finish pass if the cloud lacked it.
- [ ] Reporter updates follow `reporter-updates` only at merge or when the owner decides.
