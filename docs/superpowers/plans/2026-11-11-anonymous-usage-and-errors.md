# Anonymous Usage Counts and Error Reports Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The launcher sends anonymous error reports and one daily usage ping to the Perland engine, with one switch in Settings, and nothing that identifies a player.

**Architecture:** `launcher-core::telemetry` holds the wire types, the scrubber, the on-disk state and the HTTP sender, all pure or wiremock-testable. `launcher-app::telemetry` owns one `Telemetry` handle (gate, state, background sends) that `AppCore`, a `ReportingHost` wrapper and the panic hook feed. `src-tauri` only constructs and connects them. The UI adds one toggle, one first-run line and a global error listener that calls one IPC command.

**Tech Stack:** Rust (tokio, reqwest, serde, regex, wiremock), React + Vitest.

**Spec:** `docs/superpowers/specs/2026-11-11-anonymous-usage-and-errors-design.md`

## Global Constraints

- The wire contract in the spec is frozen: field names, types and limits exactly as written.
- No identifier on disk: `telemetry.json` holds only `installWeek`, `lastPing` (a date), `counters` and queued `reports`.
- Nothing in a payload may hold the home path, OS user name, an account or player name, an email, a token, an invite token, a server address or name, or a file path outside placeholders.
- Every send is fire-and-forget on a background task with a 10 s timeout; failures are silent (a `debug` log line at most) and never become a `LauncherError` or an event.
- Off means off: with the switch off, a dev build, `PERLAND_LAUNCHER_TELEMETRY=0`, or `PERLAND_LAUNCHER_ROOT` without an explicit endpoint, nothing is sent and nothing is recorded.
- `SHARE_BY_DEFAULT` is the one constant behind the switch's default.
- Every UI string in `en.json` and `es.json`; the IPC contract is regenerated, never edited.
- No fuzzing, stress or load tests locally.

## Review Focus

- A clock that jumps backwards, then forwards: no second ping for a day already pinged, no negative counter (Task 3).
- A queued report that the engine rejects with 400 forever: dropped, never retried (Task 4).
- A panic while the state lock is held (a panic inside telemetry itself): the hook must not deadlock (Task 5).
- A UI error loop firing hundreds of times a second: at most a handful of IPC calls per session (Task 7).
- A damaged or hand-edited `telemetry.json`: treated as empty, never an error to the player (Task 3).

---

### Task 1: The switch in settings

**Files:** Modify `crates/launcher-core/src/settings.rs`; create `crates/launcher-core/src/telemetry/mod.rs`; modify `crates/launcher-core/src/lib.rs`.

**Produces:** `telemetry::SHARE_BY_DEFAULT: bool = true`; `Settings::share_anonymous_data: bool` (JSON `shareAnonymousData`, default `SHARE_BY_DEFAULT`).

- [ ] Test: defaults carry `SHARE_BY_DEFAULT`; an old file without the field reads as the default; `false` round-trips.
- [ ] Implement, run `cargo test -p launcher-core settings`, commit.

### Task 2: Wire types and the scrubber

**Files:** Create `crates/launcher-core/src/telemetry/payload.rs`, `crates/launcher-core/src/telemetry/scrub.rs`.

**Produces:**
- `ReportKind { Exception, Panic, UnhandledRejection, AppError, GameCrash }` (snake_case on the wire).
- `Report { kind, message, code: Option<String>, stack: Option<String>, context: BTreeMap<String,String>, count: u32 }` with `Report::new(kind, message)`, `.code()`, `.stack()`, `.context(key, value)`; every setter clamps to the contract (message 1..500 chars, code `[a-z0-9_.:-]{1,64}`, stack 8192, 16 context keys matching `^[a-zA-Z][a-zA-Z0-9_]{0,31}$`, values 200 chars, count 1..1000). `Report::same_as(&other)` ignores `count`.
- `ErrorBatch { source: "launcher", version, os, arch, locale, reports }`, `Ping { version, channel, os, arch, locale, signedIn, firstEver, firstThisWeek, firstThisMonth, installWeek, counters }`, `Client { version, os, arch, locale }` with `Client::current(version, locale)`.
- `Scrubber::new()`, `.with_home(..)`, `.with_data_root(..)`, `.with_user(..)`, `.with_secret(..)`, `Scrubber::for_current_user(data_root)`, `.scrub(&str) -> String`. Order: secrets → `<redacted>`; data root → `<data>`, home → `<home>` (all three slash forms); emails → `<email>`; URLs → `https://<known host>` for an allow-list of public hosts, `<app>/path` for the webview's own origin, else `<url>`; remaining absolute paths → `<path>` (keeping what follows `registry/src/<index>/` as `<cargo>/…` and `/rustc/<hash>/` as `<rustc>/…`); `host:port` and IPv4 → `<address>`; the OS user name as a whole word → `<user>`.
- `report_for_error(&LauncherError, command: Option<&str>, &Scrubber) -> Report` (kind `app_error`, `code`, scrubbed `message`, `context.category`, `context.detail`, `context.command`, allow-listed params as `p_<key>`: `version`, `loader`, `loader_version`, `status`, `item`, `field`, `step`, `method`, `component`, `reason`, `what`, `min`, `max`, `count`, and `host` only when it is a known public host).

- [ ] Tests (`scrub.rs`): Windows home in `\`, `\\` and `/` forms; Linux home; a path outside home; a path with a space; cargo registry and rustc frames keep their tail; URL with a query token; unknown host; known host; webview stack frame; email; `play.example.net:25565`; IPv4; user name as a word but not inside another word; a secret shorter than 4 characters is matched as a whole word only.
- [ ] Tests (`payload.rs`): clamping of every field; multi-byte text is cut on a character boundary; the exact JSON shape of an `ErrorBatch` and a `Ping` (field names as in the spec); `report_for_error` on realistic errors (a download error with a URL in detail, a disk error with a path, an error whose params hold `name`, `id`, `file`, `address`, `host`) never contains the planted home, user, account name, token, email, address or path.
- [ ] Implement, run, commit.

### Task 3: The state file

**Files:** Create `crates/launcher-core/src/telemetry/state.rs`, `crates/launcher-core/src/telemetry/date.rs`.

**Produces:**
- `date::Day { year, month, day }` with `Day::from_unix(secs)`, `.iso_week() -> (i32, u32)`, `.week_label() -> String` (`YYYY-Www`), `Ord`, serde as `YYYY-MM-DD`; `install_week(day) -> String` clamps to `2026-W40`.
- `Counter` enum with `as_str()` and `ALL`: `GameLaunches`, `InstancesCreated`, `PacksInstalled`, `JoinsByLink`, `PerlandSignIns`, `MicrosoftSignIns`, `FreeServersCreated`, `SkinsChanged`, `CrashFixesApplied`, `UpdatesInstalled`.
- `State { install_week: Option<String>, last_ping: Option<Day>, counters: BTreeMap<String,u32>, reports: Vec<Report> }`; `State::load(path)` (missing or damaged → empty), `.save(path)` (blocking, atomic); `.push(report)` (collapse identical by adding `count`, cap 50, the oldest dropped), `.count(counter)` (saturating at 100000), `.ping_due(today) -> Option<PingFlags { first_ever, first_this_week, first_this_month }>` (None unless `today > last_ping`), `.ping_delivered(today, &sent_counters)` (subtracts what was sent), `.take_batches(max_bytes) -> Vec<Vec<Report>>` (≤20 reports and ≤ 60 KiB each), `.forget(&[Report])`, `.clear_collected()` (reports and counters go; dates stay).
- `QUEUE_CAP = 50`, `STATE_FILE = "telemetry.json"`.

- [ ] Tests: ISO weeks at year edges (2026-12-31 → 2026-W53, 2027-01-03 → 2026-W53, 2027-01-04 → 2027-W01, 2024-12-30 → 2025-W01); install week clamp; ping flags for same day, next day same week, next week same month, next month, first ever; clock backwards gives `None`; counters subtract and never go below zero; queue collapses and caps; batches respect both limits; a damaged file loads empty; the saved file's top-level keys are exactly `version`, `installWeek`, `lastPing`, `counters`, `reports`.
- [ ] Implement, run, commit.

### Task 4: The sender

**Files:** Create `crates/launcher-core/src/telemetry/send.rs`; test `crates/launcher-core/tests/telemetry.rs`.

**Produces:** `Outcome { Delivered, Rejected, Retry }`; `Sender::new(http: HttpClient)`, `.with_timeout(Duration)` (default 10 s); `.errors(engine: &Url, batch: &ErrorBatch) -> Outcome`; `.ping(engine: &Url, ping: &Ping) -> Outcome`. 2xx → Delivered; 400/413 → Rejected; everything else, a timeout and a network error → Retry. Never returns an error.

- [ ] Wiremock tests: paths and JSON bodies exactly as the spec; the launcher's user agent and no `authorization` header or cookie; 202, 200, 400, 413, 429, 500, a closed port and a response slower than the timeout map to the right `Outcome`.
- [ ] Implement, run, commit.

### Task 5: The `Telemetry` handle

**Files:** Create `crates/launcher-app/src/telemetry.rs`; modify `crates/launcher-app/src/lib.rs`.

**Produces:**
- `Mode { Off, Engine(Url), Site(Url) }` and `mode(build, switch: Option<&str>, scratch_root: bool, endpoint: Option<&str>, site: Url) -> Mode`: switch `0`/`false`/`off`/`no` → Off; a usable endpoint → `Engine`; a dev build or a scratch root → Off; else `Site`.
- `Telemetry::new(TelemetryConfig { mode, path, version, channel, http, scrubber_root })`, `Telemetry::off()`, `.set_sharing(bool)` (off deletes collected data at once), `.is_on()`, `.count(Counter)`, `.report(Report)`, `.report_error(&LauncherError, command)`, `.report_ui(kind, message, stack)`, `.report_panic(message, location, backtrace)`, `.add_names(impl IntoIterator<Item = String>)` (strings to mask), `.start(signed_in: bool, locale)` (ping if due, then flush), `.flush()`.
- `install_panic_hook(Arc<Telemetry>)`: records with `try_lock`, then calls the previous hook.
- Codes never reported: `SignInCanceled`, `SyncCancelled`, `ImportCancelled`.

- [ ] Tests (wiremock engine + tempdir): off in every off mode sends nothing and writes no file; on, an error is queued then delivered and forgotten; 400 drops, 500 keeps; turning sharing off empties the file's reports and counters and later reports are not recorded; the ping is sent once per day and carries the counters, then they are zero; a failed ping keeps counters and `lastPing`; a ping body has exactly the contract's keys; a whole payload built from errors with planted private strings holds none of them; the panic hook records a report while the state lock is free and returns while it is held.
- [ ] Implement, run, commit.

### Task 6: Wiring

**Files:** Modify `crates/launcher-app/src/app.rs` (config field, `settings_set`, `telemetry_begin`, `telemetry_report_ui`), `events.rs`/new `ReportingHost` in `telemetry.rs`, the counter sites (`runs.rs`, `packs.rs`, `servers.rs`, `perland.rs`, `msa.rs`, `free_servers.rs`, `friends.rs`, `skins.rs`, `crash_fixes.rs`, `updates.rs`), `src-tauri/src/{main,commands}.rs`, `crates/launcher-app/tests/contract.rs`.

**Produces:** `AppConfig::telemetry: Arc<Telemetry>`; `AppCore::telemetry()`; `ReportingHost<H>` reporting `Warning` errors, a run's `exit.error`, a crash's cause (`game_crash`, code = cause code or `unknown`, context `minecraft`, `loader`) and counting `game_launches` on `Running`; IPC command `telemetry_report { kind, message, stack }`; `logged()` in `commands.rs` reports through a handle set once at start.

- [ ] Tests in `launcher-app`: each counter site counts once on success and not on failure; `settings_set` with the switch off empties the queue; a `Warning` event and a failed run are reported; a crash report carries no log text; `telemetry_report` rejects unknown kinds silently.
- [ ] Regenerate the contract (`UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`), commit.

### Task 7: UI

**Files:** Modify `ui/src/features/settings/Settings.tsx`, `ui/src/features/first-run/FirstRun.tsx`, `ui/src/ipc/{schema,commands,mock}.ts`, `ui/src/main.tsx`, `ui/src/i18n/{en,es}.json`; create `ui/src/state/reportErrors.ts` (+ test).

**Produces:** the Settings row "Share anonymous data" with its explanation and a "What is sent" link; the first-run line with the same link; `installErrorReporting()` listening to `error` and `unhandledrejection`, deduplicating and capped at 10 calls per session.

- [ ] Vitest: toggling saves `shareAnonymousData`; the link opens the README section; first run shows the line; the reporter caps and deduplicates and never throws when the command fails.
- [ ] Style through `/impeccable`, run UI lint, typecheck and tests, commit.

### Task 8: README, release note, spec amendments

- [ ] README section "What the launcher sends" (every field, when, where, how to turn it off, what is never sent, each counter).
- [ ] `release-notes/anonymous-reports.json`; `pnpm --filter release-scripts test`.
- [ ] Amend the spec's "Calls made while building". Run every pre-commit check from CLAUDE.md, commit, push, open the draft PR.
