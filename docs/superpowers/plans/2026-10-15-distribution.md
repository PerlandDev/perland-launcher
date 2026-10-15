# Distribution (Plan 6) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the Perland Launcher as a downloadable closed beta: per-user Windows NSIS and Linux AppImage/`.deb` installers that register `perland://`, Ed25519-signed auto-updates from GitHub Releases on stable and beta channels, a launcher log with Export diagnostics, and the beta download on perland.online/launcher.

**Architecture:** A compile-time `BuildKind::Beta` keeps offline accounts open in beta builds. A Tauri-free update state machine in `launcher-app` (`updates.rs`) sits behind an `UpdateSource` trait, which `src-tauri` implements over `tauri-plugin-updater`. A one-click `release.yml` workflow computes the version, builds and signs the bundles, publishes a versioned GitHub Release, and refreshes two rolling channel releases (`channel-beta`, `channel-stable`) that hold `latest.json` and fixed-name installers. Logging (`tracing`) and diagnostics live in `launcher-core`.

**Tech Stack:** Rust (tauri 2.12, tauri-plugin-updater 2.x, tracing, tracing-subscriber, tracing-appender, zip 2, windows-sys), React + TypeScript + zod + Vitest, GitHub Actions, Node 24 `node:test`, Astro (PerlandDev-site).

**Spec:** `docs/superpowers/specs/2026-10-14-distribution-design.md` (read it with this plan; §-references below point into it).

## Global Constraints

- `launcher-core` and `launcher-app` never depend on Tauri or UI crates. `src-tauri` is wiring only and has no tests.
- TDD: the failing test comes first. Pure logic gets `#[cfg(test)]` unit tests; HTTP behaviour gets wiremock tests; UI gets Vitest + Testing Library.
- Never run fuzzing, stress or load tests locally. Keep local test inputs tiny (byte caps in tests are a few bytes, never a large body).
- Player-facing errors are `LauncherError` with a stable `ErrorCode`. Every new code gets `errors.<code>.message` (and a `fix` where it has one) in both `ui/src/i18n/en.json` and `ui/src/i18n/es.json`. `ui/src/i18n/i18n.test.ts` fails otherwise.
- Every UI string goes in both `en.json` and `es.json` (neutral Latin American Spanish, informal "tú").
- `ui/src/ipc/contract/*.json` is generated: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit it.
- UI styling follows `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md` (one lit button per view, lime only for selection, blue for progress). Task 12 runs the `/impeccable` pass.
- Tokens, secrets and the private signing key never reach logs, the repo or chat. Only the updater **public** key is committed.
- New error codes are named `launcher_update_failed`, `launcher_update_invalid`, `launcher_update_busy`, `mrpack_corrupt`, `diagnostics_path_invalid`.
- Update endpoints: `https://github.com/PerlandDev/perland-launcher/releases/download/channel-beta/latest.json` and `…/channel-stable/latest.json`.
- Manifest platform keys: `windows-x86_64-nsis`, `linux-x86_64-appimage`, `linux-x86_64-deb`.
- Workflows: `permissions: contents: read` at the top, `contents: write` only on the publish job, `timeout-minutes` on every job.
- Commit messages end with exactly this trailer (pin it in every subagent prompt):
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
- Pre-commit gate before the PR: `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.
- Windows Smart App Control sometimes blocks unsigned test executables (os error 4551). That is environmental: retry.
- A parallel session ("Fix beta feedback: launcher issues #4–#10") may change `tauri.conf.json`, `main.rs`, the instance rail and rename. Keep `tauri.conf.json` edits to `bundle`, `plugins.updater` and dropping `version`, and `main.rs` edits to the deep-link registration, updater, logging and exit hook. Rebase onto `main` before the PR (Task 18).

## Owner prerequisites

- **P1 (before Task 5):** the owner runs `pnpm tauri signer generate -w ~/.tauri/perland-updater.key` on their own machine, adds the secrets `TAURI_SIGNING_PRIVATE_KEY` (the key file's contents) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` under GitHub → Settings → Secrets and variables → Actions, and pastes only the **public** key (`perland-updater.key.pub` contents) in chat. Never ask for the private key or password.
- **P2 (Tasks 19–20):** each publish is public and waits for the owner's explicit go-ahead in chat.

## Review Focus

1. **The stable channel has no release yet.** A beta player who switches to Stable makes the updater fetch a `latest.json` that 404s. They should see "Up to date", never "Couldn't check". `TauriUpdateSource::check` maps the plugin's "release not found" error to `Ok(None)` (Task 5). The end-to-end run checks it (Task 20, step 6).
2. **The channel changes while a check or download is in flight.** The new channel must win, and the old channel's download must never install. Checks are serialized and a channel change drops the ready update (Task 3 test `channel_change_waits_for_the_running_check`).
3. **Quitting mid-download.** Nothing installs, and the next start downloads again (Task 3 test `quit_during_a_download_installs_nothing`).
4. **A hand-edited `settings.json` with an unknown channel** (`"updateChannel": "nightly"`). Defaults plus the existing settings-damaged warning, never a crash (Task 2 test `unknown_channel_is_a_damaged_file`).
5. **Exporting diagnostics into a folder that doesn't exist, or onto a damaged `instance.json`.** A clear disk error with no `.part` file left behind, and a damaged instance file is replaced by a note rather than copied raw (Task 10 tests `missing_folder_fails_cleanly` and `a_damaged_instance_file_is_left_out`).

---

### Task 1: Beta build kind

**Files:**
- Modify: `crates/launcher-core/src/accounts.rs:99-118` (BuildKind, offline gate, tests ~263-281)
- Modify: `crates/launcher-app/src/dto.rs:22-35` (Build)
- Modify: `crates/launcher-app/src/app.rs` (test `app_info_reflects_build_and_offline_gate`)
- Modify: `ui/src/ipc/schema.ts:106-112`, `ui/src/ipc/mock.ts` (MockOptions, `app_info`), `ui/src/features/settings/Settings.tsx:105`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Test: `ui/src/features/settings/Settings.test.tsx`

**Interfaces:**
- Produces: `BuildKind::{Dev, Beta, Release}`; `BuildKind::from_parts(debug_assertions: bool, flag: Option<&str>) -> BuildKind`; `BuildKind::current()`; `offline_allowed(BuildKind, bool) -> bool` (true for Dev and Beta); IPC `Build::Beta` serialized `"beta"`; mock option `beta?: boolean`.

- [ ] **Step 1: Write the failing core tests** in `accounts.rs` `mod tests` (replace `offline_gate`, add the others):

```rust
#[test]
fn build_kind_comes_from_debug_assertions_then_the_beta_flag() {
    assert_eq!(BuildKind::from_parts(true, None), BuildKind::Dev);
    assert_eq!(BuildKind::from_parts(true, Some("beta")), BuildKind::Dev);
    assert_eq!(BuildKind::from_parts(false, Some("beta")), BuildKind::Beta);
    assert_eq!(BuildKind::from_parts(false, None), BuildKind::Release);
    assert_eq!(BuildKind::from_parts(false, Some("")), BuildKind::Release);
    assert_eq!(BuildKind::from_parts(false, Some("Beta")), BuildKind::Release);
}

#[test]
fn offline_gate() {
    assert!(offline_allowed(BuildKind::Dev, false));
    assert!(offline_allowed(BuildKind::Beta, false));
    assert!(!offline_allowed(BuildKind::Release, false));
    assert!(offline_allowed(BuildKind::Release, true));
}

#[tokio::test]
async fn beta_builds_add_offline_accounts() {
    let tmp = tempfile::tempdir().unwrap();
    let store = AccountStore::new(tmp.path().join("accounts.json"));
    let added = store.add_offline("Steve", BuildKind::Beta).await.unwrap();
    assert_eq!(added.username, "Steve");
}
```

(Check `AccountStore::new`'s argument and the `Account` field name against the existing `add_offline` tests at lines ~270-281 and match them.)

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p launcher-core accounts::`. Expected: a compile error (no `Beta` variant, no `from_parts`).

- [ ] **Step 3: Implement**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKind {
    Dev,
    /// An optimized build made with `PERLAND_BUILD=beta`: offline accounts stay open (spec §3.1).
    Beta,
    Release,
}

impl BuildKind {
    pub fn current() -> Self {
        Self::from_parts(cfg!(debug_assertions), option_env!("PERLAND_BUILD"))
    }

    /// Debug assertions make a dev build; otherwise `PERLAND_BUILD=beta` at compile time makes
    /// a beta. rustc tracks `option_env!`, so changing the variable rebuilds this crate.
    pub fn from_parts(debug_assertions: bool, flag: Option<&str>) -> Self {
        if debug_assertions {
            Self::Dev
        } else if flag == Some("beta") {
            Self::Beta
        } else {
            Self::Release
        }
    }
}

/// Dev and beta builds allow offline accounts; release builds unlock them only after a licensed
/// Microsoft sign-in on this install.
pub fn offline_allowed(build: BuildKind, licensed_signin_seen: bool) -> bool {
    matches!(build, BuildKind::Dev | BuildKind::Beta) || licensed_signin_seen
}
```

In `dto.rs` add `Beta` to `Build` (between `Dev` and `Release`) and `BuildKind::Beta => Build::Beta` to the `From` impl.

- [ ] **Step 4: Extend the app test.** In `app.rs` `app_info_reflects_build_and_offline_gate`, add:

```rust
let (beta, _, _, _t3) = core(BuildKind::Beta);
let info = beta.app_info().await.unwrap();
assert_eq!(info.build, Build::Beta);
assert!(info.offline_allowed);
beta.account_add_offline("Steve").await.unwrap();
```

Run `cargo test -p launcher-core accounts:: && cargo test -p launcher-app app::tests::app_info`. Expected: PASS.

- [ ] **Step 5: Write the failing UI test** in `Settings.test.tsx`:

```tsx
it('labels a beta build in About', async () => {
  renderApp(<Settings />, { beta: true });
  expect(await screen.findByText(/Version .* · Beta build/)).toBeInTheDocument();
});
```

Run `pnpm --filter ui test -- Settings`. Expected: FAIL (the `beta` option is unknown, so there's no label).

- [ ] **Step 6: Implement the UI.**
  - `schema.ts`: `build: z.enum(['dev', 'beta', 'release'])`.
  - `mock.ts`: add `beta?: boolean` to `MockOptions`, and in `app_info` use `build: options.release ? 'release' : options.beta ? 'beta' : 'dev'`.
  - `Settings.tsx` next to the dev label:

    ```tsx
    {appInfo.data.build === 'beta' && ` · ${t('settings.betaBuild')}`}
    ```

  - `en.json` → `settings.betaBuild`: `"Beta build"`; `es.json` → `"Versión beta"`.

Run `pnpm --filter ui test`. Expected: PASS.

- [ ] **Step 7: Commit**:

```bash
git add crates/launcher-core/src/accounts.rs crates/launcher-app/src/dto.rs crates/launcher-app/src/app.rs ui/src
git commit -m "feat: beta build kind keeps offline accounts open without debug assertions

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Update channel setting

**Files:**
- Modify: `crates/launcher-core/src/settings.rs` (Channel, `Settings.update_channel`, tests)
- Modify: `ui/src/ipc/schema.ts:99-105` (settings)
- Regenerate: `ui/src/ipc/contract/settings.json`

**Interfaces:**
- Consumes: `BuildKind` (Task 1).
- Produces: `launcher_core::settings::Channel::{Stable, Beta}` (serde `"stable"`/`"beta"`); `Channel::effective(choice: Option<Channel>, build: BuildKind) -> Channel`; `Settings.update_channel: Option<Channel>` (JSON `updateChannel`, default `null`); UI `settings.updateChannel: 'stable' | 'beta' | null`.

- [ ] **Step 1: Write the failing tests** in `settings.rs` `mod tests`:

```rust
use crate::accounts::BuildKind;

#[test]
fn channel_follows_the_build_until_the_player_picks() {
    assert_eq!(Channel::effective(None, BuildKind::Beta), Channel::Beta);
    assert_eq!(Channel::effective(None, BuildKind::Release), Channel::Stable);
    assert_eq!(Channel::effective(None, BuildKind::Dev), Channel::Stable);
    assert_eq!(Channel::effective(Some(Channel::Stable), BuildKind::Beta), Channel::Stable);
    assert_eq!(Channel::effective(Some(Channel::Beta), BuildKind::Release), Channel::Beta);
}

#[tokio::test]
async fn an_old_file_without_a_channel_reads_as_none() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("settings.json");
    std::fs::write(&path, r#"{"version":1,"language":"en","defaultMemoryMb":4096,"minimizeOnPlay":true,"firstRunDone":true}"#).unwrap();
    let (settings, warning) = SettingsStore::new(&path).load().await;
    assert!(warning.is_none());
    assert_eq!(settings.update_channel, None);
}

#[tokio::test]
async fn channel_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(tmp.path().join("settings.json"));
    let settings = Settings { update_channel: Some(Channel::Beta), ..Settings::default() };
    store.save(&settings).await.unwrap();
    let raw = std::fs::read_to_string(tmp.path().join("settings.json")).unwrap();
    assert!(raw.contains("\"updateChannel\": \"beta\""), "{raw}");
    assert_eq!(store.load().await, (settings, None));
}

#[tokio::test]
async fn unknown_channel_is_a_damaged_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("settings.json");
    std::fs::write(&path, r#"{"updateChannel":"nightly"}"#).unwrap();
    let (settings, warning) = SettingsStore::new(&path).load().await;
    assert_eq!(settings, Settings::default());
    assert_eq!(warning.unwrap().code, ErrorCode::SettingsDamaged);
}
```

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p launcher-core settings::`. Expected: a compile error (no `Channel`).

- [ ] **Step 3: Implement** in `settings.rs`:

```rust
use crate::accounts::BuildKind;

/// Which launcher releases this install follows (spec §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
}

impl Channel {
    /// The player's pick, else the build's own channel: beta builds follow beta, the rest stable.
    pub fn effective(choice: Option<Channel>, build: BuildKind) -> Channel {
        choice.unwrap_or(if build == BuildKind::Beta {
            Channel::Beta
        } else {
            Channel::Stable
        })
    }
}
```

Add to `Settings` (after `first_run_done`), with a doc comment:

```rust
    /// `None` until the player picks one; then it follows the build (`Channel::effective`).
    pub update_channel: Option<Channel>,
```

Add `update_channel: None,` to `Default`. The struct already has `#[serde(default)]`, so old files load.

- [ ] **Step 4: Run the tests.** Run `cargo test -p launcher-core settings::`. Expected: PASS.

- [ ] **Step 5: Update the UI schema and contract.** In `schema.ts` `settings` add `updateChannel: z.enum(['stable', 'beta']).nullable(),` and export `export type Channel = 'stable' | 'beta';`. Then:

```bash
UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract
cargo test -p launcher-app --test contract
pnpm --filter ui typecheck
pnpm --filter ui test
```

Expected: `settings.json` gains `"updateChannel": null` and everything passes. Fix any `Settings` object literals in UI tests that `typecheck` reports by adding `updateChannel: null`.

- [ ] **Step 6: Commit** with message `feat: update channel setting (stable/beta, follows the build by default)` and the trailer.

---

### Task 3: Launcher update state machine

**Files:**
- Create: `crates/launcher-app/src/updates.rs`
- Modify: `crates/launcher-app/src/lib.rs` (add `pub mod updates;`)
- Modify: `crates/launcher-app/src/events.rs` (`AppEvent::Update`)
- Modify: `crates/launcher-core/src/error.rs` (3 codes, test)
- Modify: `crates/launcher-app/tests/contract.rs` (events sample, `update_states`)
- Modify: `ui/src/ipc/schema.ts` (`updateState`, `appEvent`), `ui/src/ipc/contract.test.ts`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json` (errors)
- Regenerate: `ui/src/ipc/contract/{events,error_codes,update_states}.json`

**Interfaces:**
- Consumes: `Channel` (Task 2), `BuildKind` (Task 1), `crate::runner::BoxFuture<T>` (`Pin<Box<dyn Future<Output = T> + Send + 'static>>`), `Host`, `RecordingHost` (tests).
- Produces:
  - `updates::{BETA_ENDPOINT, STABLE_ENDPOINT, START_DELAY, endpoint(Channel) -> &'static str}`.
  - `updates::UpdateState` (serde `tag = "state"`, snake_case: `off`, `idle`, `checking`, `downloading { percent: Option<u8> }`, `ready { version }`, `up_to_date`, `failed { error }`).
  - `updates::Progress = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>`, where the first argument is the chunk length.
  - `trait UpdateSource { type Update; fn check(&self, &'static str) -> BoxFuture<Result<Option<Self::Update>>>; fn version(&self, &Self::Update) -> String; fn download(&self, Arc<Self::Update>, Progress) -> BoxFuture<Result<Vec<u8>>>; fn install(&self, Arc<Self::Update>, Arc<Vec<u8>>, relaunch: bool) -> BoxFuture<Result<()>>; }`.
  - `Updates<S, H>`:
    - `new(Arc<S>, Arc<H>, BuildKind, Channel) -> Arc<Self>`;
    - `status()`;
    - `async start(delay)`, `async check() -> UpdateState`, `async set_channel(Channel) -> UpdateState`;
    - `async restart_now(busy: bool) -> Result<()>`, `async on_quit(busy: bool) -> bool`.
  - `updates::busy_error() -> LauncherError`.
  - `AppEvent::Update { state: UpdateState }` (wire `{"type":"update","state":{...}}`).
  - `ErrorCode::{LauncherUpdateFailed, LauncherUpdateInvalid, LauncherUpdateBusy}`.

- [ ] **Step 1: Add the error codes.** First write the failing test in `error.rs` `mod tests`:

```rust
#[test]
fn launcher_update_codes_exist() {
    for (code, wire) in [
        (ErrorCode::LauncherUpdateFailed, "launcher_update_failed"),
        (ErrorCode::LauncherUpdateInvalid, "launcher_update_invalid"),
        (ErrorCode::LauncherUpdateBusy, "launcher_update_busy"),
    ] {
        assert_eq!(serde_json::to_value(code).unwrap(), wire);
        assert!(ErrorCode::ALL.contains(&code));
    }
}
```

Run `cargo test -p launcher-core error::`: FAIL. Add `// launcher updates` followed by `LauncherUpdateFailed, LauncherUpdateInvalid, LauncherUpdateBusy,` to `error_codes!` before `// anything unexpected`. Run again: PASS.

- [ ] **Step 2: Write `updates.rs` with its failing tests first.** Create the file with only the test module and type stubs (the bodies come in Step 4). The tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::RecordingHost;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::Semaphore;

    #[derive(Default)]
    struct FakeSource {
        found: StdMutex<Option<Result<Option<String>>>>,
        download: StdMutex<Option<Result<Vec<u8>>>>,
        chunks: Vec<(u64, Option<u64>)>,
        endpoints: StdMutex<Vec<&'static str>>,
        installs: StdMutex<Vec<(String, Vec<u8>, bool)>>,
        /// When set, check waits for one permit (to hold a check in flight).
        check_gate: Option<Arc<Semaphore>>,
        /// When set, download waits for one permit.
        download_gate: Option<Arc<Semaphore>>,
    }

    impl FakeSource {
        fn offering(version: &str) -> Self {
            Self { found: StdMutex::new(Some(Ok(Some(version.to_string())))), ..Self::default() }
        }
    }

    impl UpdateSource for FakeSource {
        type Update = String;
        fn check(&self, endpoint: &'static str) -> BoxFuture<Result<Option<String>>> {
            self.endpoints.lock().unwrap().push(endpoint);
            let result = self.found.lock().unwrap().clone().unwrap_or(Ok(None));
            let gate = self.check_gate.clone();
            Box::pin(async move {
                if let Some(gate) = gate {
                    gate.acquire().await.unwrap().forget();
                }
                result
            })
        }
        fn version(&self, update: &String) -> String {
            update.clone()
        }
        fn download(&self, _update: Arc<String>, progress: Progress) -> BoxFuture<Result<Vec<u8>>> {
            for (chunk, total) in &self.chunks {
                progress(*chunk, *total);
            }
            let result = self.download.lock().unwrap().clone().unwrap_or_else(|| Ok(b"setup".to_vec()));
            let gate = self.download_gate.clone();
            Box::pin(async move {
                if let Some(gate) = gate {
                    gate.acquire().await.unwrap().forget();
                }
                result
            })
        }
        fn install(&self, update: Arc<String>, bytes: Arc<Vec<u8>>, relaunch: bool) -> BoxFuture<Result<()>> {
            self.installs.lock().unwrap().push(((*update).clone(), (*bytes).clone(), relaunch));
            Box::pin(async { Ok(()) })
        }
    }

    fn states(host: &RecordingHost) -> Vec<UpdateState> {
        host.events.lock().unwrap().iter().filter_map(|e| match e {
            AppEvent::Update { state } => Some(state.clone()),
            _ => None,
        }).collect()
    }

    fn ready(version: &str) -> UpdateState {
        UpdateState::Ready { version: version.into() }
    }

    fn net_error() -> LauncherError {
        LauncherError::network(ErrorCode::LauncherUpdateFailed, "Couldn't check for launcher updates")
    }

    #[tokio::test]
    async fn dev_builds_are_off_and_never_check() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let host = RecordingHost::new();
        let updates = Updates::new(source.clone(), host.clone(), BuildKind::Dev, Channel::Beta);
        assert_eq!(updates.status(), UpdateState::Off);
        assert_eq!(updates.check().await, UpdateState::Off);
        updates.start(Duration::ZERO).await;
        assert!(source.endpoints.lock().unwrap().is_empty());
        assert!(states(&host).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn start_waits_then_downloads_and_is_ready() {
        let source = Arc::new(FakeSource {
            chunks: vec![(50, Some(100)), (50, Some(100))],
            ..FakeSource::offering("0.2.0-beta.2")
        });
        let host = RecordingHost::new();
        let updates = Updates::new(source.clone(), host.clone(), BuildKind::Beta, Channel::Beta);
        assert_eq!(updates.status(), UpdateState::Idle);
        let started = tokio::time::Instant::now();
        updates.start(START_DELAY).await;
        assert!(started.elapsed() >= START_DELAY);
        assert_eq!(*source.endpoints.lock().unwrap(), vec![BETA_ENDPOINT]);
        assert_eq!(states(&host), vec![
            UpdateState::Checking,
            UpdateState::Downloading { percent: Some(0) },
            UpdateState::Downloading { percent: Some(50) },
            UpdateState::Downloading { percent: Some(100) },
            ready("0.2.0-beta.2"),
        ]);
        assert_eq!(updates.status(), ready("0.2.0-beta.2"));
    }

    #[tokio::test]
    async fn nothing_newer_is_up_to_date() {
        let source = Arc::new(FakeSource::default());
        let host = RecordingHost::new();
        let updates = Updates::new(source, host.clone(), BuildKind::Release, Channel::Stable);
        assert_eq!(updates.check().await, UpdateState::UpToDate);
        assert_eq!(states(&host), vec![UpdateState::Checking, UpdateState::UpToDate]);
    }

    #[tokio::test]
    async fn a_failed_check_is_failed_and_quiet() {
        let source = Arc::new(FakeSource { found: StdMutex::new(Some(Err(net_error()))), ..FakeSource::default() });
        let host = RecordingHost::new();
        let updates = Updates::new(source, host.clone(), BuildKind::Beta, Channel::Beta);
        assert_eq!(updates.check().await, UpdateState::Failed { error: net_error() });
        assert!(host.warnings().is_empty(), "no warning toast on start");
    }

    #[tokio::test]
    async fn a_failed_download_is_failed_and_installs_nothing() {
        let source = Arc::new(FakeSource { download: StdMutex::new(Some(Err(net_error()))), ..FakeSource::offering("0.2.0-beta.2") });
        let updates = Updates::new(source.clone(), RecordingHost::new(), BuildKind::Beta, Channel::Beta);
        assert!(matches!(updates.check().await, UpdateState::Failed { .. }));
        assert!(!updates.on_quit(false).await);
        assert!(source.installs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_recheck_keeps_the_ready_update() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(source.clone(), RecordingHost::new(), BuildKind::Beta, Channel::Beta);
        updates.check().await;
        *source.found.lock().unwrap() = Some(Err(net_error()));
        assert_eq!(updates.check().await, ready("0.2.0-beta.2"));
    }

    #[tokio::test]
    async fn the_same_version_isnt_downloaded_twice() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let host = RecordingHost::new();
        let updates = Updates::new(source, host.clone(), BuildKind::Beta, Channel::Beta);
        updates.check().await;
        host.events.lock().unwrap().clear();
        assert_eq!(updates.check().await, ready("0.2.0-beta.2"));
        assert_eq!(states(&host), vec![UpdateState::Checking, ready("0.2.0-beta.2")]);
    }

    #[tokio::test]
    async fn restart_now_is_refused_while_busy() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(source.clone(), RecordingHost::new(), BuildKind::Beta, Channel::Beta);
        updates.check().await;
        assert_eq!(updates.restart_now(true).await.unwrap_err().code, ErrorCode::LauncherUpdateBusy);
        assert!(source.installs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn restart_now_installs_and_relaunches() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(source.clone(), RecordingHost::new(), BuildKind::Beta, Channel::Beta);
        assert_eq!(updates.restart_now(false).await.unwrap_err().code, ErrorCode::LauncherUpdateFailed);
        updates.check().await;
        updates.restart_now(false).await.unwrap();
        assert_eq!(*source.installs.lock().unwrap(), vec![("0.2.0-beta.2".to_string(), b"setup".to_vec(), true)]);
    }

    #[tokio::test]
    async fn quit_installs_without_relaunching_unless_busy() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(source.clone(), RecordingHost::new(), BuildKind::Beta, Channel::Beta);
        assert!(!updates.on_quit(false).await, "nothing ready yet");
        updates.check().await;
        assert!(!updates.on_quit(true).await, "a game still runs");
        assert!(source.installs.lock().unwrap().is_empty());
        assert!(updates.on_quit(false).await);
        assert_eq!(*source.installs.lock().unwrap(), vec![("0.2.0-beta.2".to_string(), b"setup".to_vec(), false)]);
    }

    #[tokio::test]
    async fn quit_during_a_download_installs_nothing() {
        let gate = Arc::new(Semaphore::new(0));
        let source = Arc::new(FakeSource { download_gate: Some(gate.clone()), ..FakeSource::offering("0.2.0-beta.2") });
        let updates = Updates::new(source.clone(), RecordingHost::new(), BuildKind::Beta, Channel::Beta);
        let running = { let u = updates.clone(); tokio::spawn(async move { u.check().await }) };
        while !matches!(updates.status(), UpdateState::Downloading { .. }) { tokio::task::yield_now().await; }
        assert!(!updates.on_quit(false).await);
        gate.add_permits(1);
        running.await.unwrap();
        assert!(source.installs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn channel_change_drops_the_ready_update_and_checks_the_new_endpoint() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(source.clone(), RecordingHost::new(), BuildKind::Beta, Channel::Beta);
        updates.check().await;
        *source.found.lock().unwrap() = Some(Ok(None));
        assert_eq!(updates.set_channel(Channel::Stable).await, UpdateState::UpToDate);
        assert_eq!(*source.endpoints.lock().unwrap(), vec![BETA_ENDPOINT, STABLE_ENDPOINT]);
        assert!(!updates.on_quit(false).await, "the beta download is gone");
        // The same channel again doesn't check.
        updates.set_channel(Channel::Stable).await;
        assert_eq!(source.endpoints.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn channel_change_waits_for_the_running_check() {
        let gate = Arc::new(Semaphore::new(0));
        let source = Arc::new(FakeSource { check_gate: Some(gate.clone()), ..FakeSource::offering("0.2.0-beta.2") });
        let updates = Updates::new(source.clone(), RecordingHost::new(), BuildKind::Beta, Channel::Beta);
        let first = { let u = updates.clone(); tokio::spawn(async move { u.check().await }) };
        while source.endpoints.lock().unwrap().is_empty() { tokio::task::yield_now().await; }
        let switch = { let u = updates.clone(); tokio::spawn(async move { u.set_channel(Channel::Stable).await }) };
        gate.add_permits(2);
        first.await.unwrap();
        switch.await.unwrap();
        assert_eq!(*source.endpoints.lock().unwrap(), vec![BETA_ENDPOINT, STABLE_ENDPOINT]);
        // The beta download finished first, then the switch dropped it and the stable check
        // (the fake still offers the same version) downloaded again for stable.
        assert_eq!(updates.status(), ready("0.2.0-beta.2"));
    }
}
```

Run `cargo test -p launcher-app updates::`. Expected: a compile error (nothing implemented).

- [ ] **Step 3: Add the event.** In `events.rs`, add to `AppEvent` (after `DeepLink`):

```rust
    /// The launcher's own update moved on (see `updates.rs`).
    Update { state: crate::updates::UpdateState },
```

Fix any exhaustive `match` on `AppEvent` that the compiler reports (add an arm that does nothing for `Update`).

- [ ] **Step 4: Implement `updates.rs`** above the tests:

```rust
//! The launcher's own updates (spec §4): check on start, download in the background, install on
//! quit or Restart now, and never while something runs. Tauri's updater sits behind
//! `UpdateSource`, so all of this runs in tests with a fake.
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use launcher_core::{accounts::BuildKind, settings::Channel, ErrorCode, LauncherError, Result};
use serde::Serialize;

use crate::{
    events::{AppEvent, Host},
    runner::BoxFuture,
};

pub const BETA_ENDPOINT: &str =
    "https://github.com/PerlandDev/perland-launcher/releases/download/channel-beta/latest.json";
pub const STABLE_ENDPOINT: &str =
    "https://github.com/PerlandDev/perland-launcher/releases/download/channel-stable/latest.json";

/// How long after start the first check waits, so it doesn't compete with the first screen.
pub const START_DELAY: Duration = Duration::from_secs(5);

pub fn endpoint(channel: Channel) -> &'static str {
    match channel {
        Channel::Beta => BETA_ENDPOINT,
        Channel::Stable => STABLE_ENDPOINT,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateState {
    /// Dev builds: nothing is checked.
    Off,
    Idle,
    Checking,
    Downloading { percent: Option<u8> },
    /// Downloaded and verified; installs on quit or Restart now.
    Ready { version: String },
    UpToDate,
    Failed { error: LauncherError },
}

/// Called with each downloaded chunk's length and the total size when known.
pub type Progress = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Where updates come from. The real one wraps `tauri-plugin-updater` (src-tauri/src/updater.rs).
pub trait UpdateSource: Send + Sync + 'static {
    type Update: Send + Sync + 'static;
    /// A newer version at `endpoint`, if there is one.
    fn check(&self, endpoint: &'static str) -> BoxFuture<Result<Option<Self::Update>>>;
    fn version(&self, update: &Self::Update) -> String;
    /// The installer's bytes, signature already verified.
    fn download(&self, update: Arc<Self::Update>, progress: Progress) -> BoxFuture<Result<Vec<u8>>>;
    /// Run the installer. On Windows this ends the process.
    fn install(&self, update: Arc<Self::Update>, bytes: Arc<Vec<u8>>, relaunch: bool) -> BoxFuture<Result<()>>;
}

struct Downloaded<U> {
    update: Arc<U>,
    bytes: Arc<Vec<u8>>,
}

pub struct Updates<S: UpdateSource, H: Host> {
    source: Arc<S>,
    host: Arc<H>,
    enabled: bool,
    channel: Mutex<Channel>,
    state: Arc<Mutex<UpdateState>>,
    ready: Mutex<Option<Downloaded<S::Update>>>,
    /// Checks run one at a time; a channel change waits for the running one.
    flight: tokio::sync::Mutex<()>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Restart now while a game, an install or a sync runs.
pub fn busy_error() -> LauncherError {
    LauncherError::game(
        ErrorCode::LauncherUpdateBusy,
        "Close your games before restarting to update",
    )
    .with_fix("The update also installs when you close the launcher")
}

fn nothing_ready() -> LauncherError {
    LauncherError::network(ErrorCode::LauncherUpdateFailed, "No launcher update is ready yet")
}

impl<S: UpdateSource, H: Host> Updates<S, H> {
    pub fn new(source: Arc<S>, host: Arc<H>, build: BuildKind, channel: Channel) -> Arc<Self> {
        let enabled = build != BuildKind::Dev;
        Arc::new(Self {
            source,
            host,
            enabled,
            channel: Mutex::new(channel),
            state: Arc::new(Mutex::new(if enabled { UpdateState::Idle } else { UpdateState::Off })),
            ready: Mutex::new(None),
            flight: tokio::sync::Mutex::new(()),
        })
    }

    pub fn status(&self) -> UpdateState {
        lock(&self.state).clone()
    }

    fn set(&self, state: UpdateState) -> UpdateState {
        *lock(&self.state) = state.clone();
        self.host.emit(AppEvent::Update { state: state.clone() });
        state
    }

    fn ready_state(&self) -> Option<UpdateState> {
        lock(&self.ready).as_ref().map(|r| UpdateState::Ready { version: self.source.version(&r.update) })
    }

    /// A failure keeps a downloaded update installable; otherwise it is `Failed`.
    fn fail(&self, error: LauncherError) -> UpdateState {
        let state = self.ready_state().unwrap_or(UpdateState::Failed { error });
        self.set(state)
    }

    /// The check on start, after `delay`. Dev builds never check.
    pub async fn start(&self, delay: Duration) {
        if !self.enabled {
            return;
        }
        tokio::time::sleep(delay).await;
        self.check().await;
    }

    /// Look for a newer version on the current channel and download it.
    pub async fn check(&self) -> UpdateState {
        if !self.enabled {
            return UpdateState::Off;
        }
        let _flight = self.flight.lock().await;
        self.set(UpdateState::Checking);
        let channel = *lock(&self.channel);
        let update = match self.source.check(endpoint(channel)).await {
            Err(error) => return self.fail(error),
            Ok(None) => {
                let state = self.ready_state().unwrap_or(UpdateState::UpToDate);
                return self.set(state);
            }
            Ok(Some(update)) => update,
        };
        let version = self.source.version(&update);
        if self.ready_state() == Some(UpdateState::Ready { version: version.clone() }) {
            return self.set(UpdateState::Ready { version });
        }
        self.set(UpdateState::Downloading { percent: Some(0) });
        let update = Arc::new(update);
        match self.source.download(update.clone(), self.progress()).await {
            Err(error) => self.fail(error),
            Ok(bytes) => {
                *lock(&self.ready) = Some(Downloaded { update, bytes: Arc::new(bytes) });
                self.set(UpdateState::Ready { version })
            }
        }
    }

    /// Progress events, sent only when the percentage changes.
    fn progress(&self) -> Progress {
        let host = self.host.clone();
        let state = self.state.clone();
        let received = Arc::new(Mutex::new(0u64));
        Arc::new(move |chunk, total| {
            let got = {
                let mut got = lock(&received);
                *got += chunk;
                *got
            };
            let percent = total.filter(|t| *t > 0).map(|t| (got.min(t) * 100 / t) as u8);
            let next = UpdateState::Downloading { percent };
            let changed = {
                let mut current = lock(&state);
                let changed = *current != next;
                *current = next.clone();
                changed
            };
            if changed {
                host.emit(AppEvent::Update { state: next });
            }
        })
    }

    /// A new channel drops any update downloaded for the old one, then checks again.
    pub async fn set_channel(&self, channel: Channel) -> UpdateState {
        {
            let _flight = self.flight.lock().await;
            let mut current = lock(&self.channel);
            if *current == channel {
                return self.status();
            }
            *current = channel;
            *lock(&self.ready) = None;
        }
        self.check().await
    }

    /// Restart now: install and relaunch. Refused while anything runs.
    pub async fn restart_now(&self, busy: bool) -> Result<()> {
        if busy {
            return Err(busy_error());
        }
        let (update, bytes) = self.downloaded().ok_or_else(nothing_ready)?;
        self.source.install(update, bytes, true).await
    }

    /// Install a ready update as the launcher quits, unless something still runs (spec §4.3).
    /// True when the installer started.
    pub async fn on_quit(&self, busy: bool) -> bool {
        if busy {
            return false;
        }
        match self.downloaded() {
            Some((update, bytes)) => self.source.install(update, bytes, false).await.is_ok(),
            None => false,
        }
    }

    fn downloaded(&self) -> Option<(Arc<S::Update>, Arc<Vec<u8>>)> {
        lock(&self.ready).as_ref().map(|r| (r.update.clone(), r.bytes.clone()))
    }
}
```

Add `pub mod updates;` to `lib.rs`. Run `cargo test -p launcher-app updates::`. Expected: PASS. If `start_paused` complains, the dev-dependency `tokio` already has `test-util`.

- [ ] **Step 5: Contract samples.** In `tests/contract.rs`, import `launcher_app::updates::UpdateState`, add `AppEvent::Update { state: UpdateState::Ready { version: "0.2.0-beta.2".into() } }` to the `events` vec, and add:

```rust
    check(
        "update_states",
        vec![
            UpdateState::Off,
            UpdateState::Idle,
            UpdateState::Checking,
            UpdateState::Downloading { percent: Some(40) },
            UpdateState::Downloading { percent: None },
            UpdateState::Ready { version: "0.2.0-beta.2".into() },
            UpdateState::UpToDate,
            UpdateState::Failed {
                error: LauncherError::network(ErrorCode::LauncherUpdateFailed, "Couldn't check for launcher updates")
                    .with_fix("You can keep playing. It tries again next time you open the launcher"),
            },
        ],
    );
```

Run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.

- [ ] **Step 6: UI schema and strings.** In `schema.ts` (after `launcherError` is defined):

```ts
export const updateState = z.discriminatedUnion('state', [
  z.strictObject({ state: z.literal('off') }),
  z.strictObject({ state: z.literal('idle') }),
  z.strictObject({ state: z.literal('checking') }),
  z.strictObject({ state: z.literal('downloading'), percent: z.number().int().nullable() }),
  z.strictObject({ state: z.literal('ready'), version: z.string() }),
  z.strictObject({ state: z.literal('up_to_date') }),
  z.strictObject({ state: z.literal('failed'), error: launcherError }),
]);
export type UpdateState = z.infer<typeof updateState>;
```

Add `z.strictObject({ type: z.literal('update'), state: updateState }),` to `appEvent`. In `contract.test.ts`, import `updateStates from './contract/update_states.json'` and add `['update_states', s.updateState.array(), updateStates],`.

Errors in `en.json` → `errors`:

```json
"launcher_update_failed": { "message": "Couldn't check for launcher updates.", "fix": "You can keep playing. It tries again next time you open the launcher." },
"launcher_update_invalid": { "message": "The launcher update didn't pass its safety check, so it wasn't installed.", "fix": "Try again later. If it keeps happening, let us know." },
"launcher_update_busy": { "message": "Close your games before restarting to update.", "fix": "The update also installs when you close the launcher." },
```

In `es.json`:

```json
"launcher_update_failed": { "message": "No se pudo buscar actualizaciones del launcher.", "fix": "Puedes seguir jugando. Lo intenta de nuevo la próxima vez que abras el launcher." },
"launcher_update_invalid": { "message": "La actualización del launcher no pasó la verificación de seguridad, así que no se instaló.", "fix": "Vuelve a intentarlo más tarde. Si sigue pasando, avísanos." },
"launcher_update_busy": { "message": "Cierra tus juegos antes de reiniciar para actualizar.", "fix": "La actualización también se instala cuando cierras el launcher." },
```

- [ ] **Step 7: Run everything touched.** Run `cargo test -p launcher-app && cargo clippy -p launcher-app --all-targets -- -D warnings && pnpm --filter ui test && pnpm --filter ui typecheck`. Expected: PASS. `runStore.apply` must ignore an `update` event; if `typecheck` reports an exhaustive switch, add a no-op case.

- [ ] **Step 8: Commit** with message `feat(app): launcher update state machine behind an UpdateSource seam` and the trailer.

---

### Task 4: Busy rule and self-registration rule

**Files:**
- Modify: `crates/launcher-app/src/app.rs` (`update_blocked`, test)
- Modify: `crates/launcher-app/src/deep_link.rs` (`should_self_register`, test)
- Modify: `src-tauri/src/main.rs:43-47`

**Interfaces:**
- Produces: `AppCore::update_blocked(&self) -> bool`; `deep_link::should_self_register(dev: bool, windows: bool, appimage: bool) -> bool`.

- [ ] **Step 1: Write the failing tests.** In `app.rs` tests:

```rust
#[tokio::test]
async fn an_install_or_content_install_blocks_updates() {
    let (core, _, _, _t) = core(BuildKind::Beta);
    assert!(!core.update_blocked());
    let permit = core.install_slot.clone().try_acquire_owned().unwrap();
    assert!(core.update_blocked());
    drop(permit);
    core.installing.lock().unwrap().insert("asalith".into());
    assert!(core.update_blocked());
}
```

In `deep_link.rs` tests:

```rust
#[test]
fn only_dev_runs_and_appimages_register_themselves() {
    assert!(should_self_register(true, true, false));
    assert!(should_self_register(true, false, false));
    assert!(!should_self_register(false, true, false), "the NSIS installer registers it");
    assert!(!should_self_register(false, false, false), "the .deb registers it");
    assert!(should_self_register(false, false, true), "an AppImage has no install step");
}
```

Run `cargo test -p launcher-app update_blocked should_self_register`. Expected: a compile error.

- [ ] **Step 2: Implement.** In `app.rs` (in `impl AppCore`):

```rust
    /// Restart now and install-on-quit wait while a game runs, an install holds the slot, or a
    /// content install writes into an instance.
    pub fn update_blocked(&self) -> bool {
        self.has_active_runs()
            || self.install_slot.available_permits() == 0
            || !self.installing.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }
```

In `deep_link.rs`:

```rust
/// Should this run register `perland://` itself? Installers register it for installed builds
/// (NSIS on Windows, the `.deb`'s desktop file on Linux); an AppImage has no install step, and a
/// dev build has no installer.
pub fn should_self_register(dev: bool, windows: bool, appimage: bool) -> bool {
    dev || (appimage && !windows)
}
```

- [ ] **Step 3: Wire it in `main.rs`.** Replace the `#[cfg(all(debug_assertions, …))]` block with:

```rust
            #[cfg(any(windows, target_os = "linux"))]
            if launcher_app::deep_link::should_self_register(
                cfg!(debug_assertions),
                cfg!(windows),
                std::env::var_os("APPIMAGE").is_some(),
            ) {
                if let Err(e) = app.deep_link().register_all() {
                    eprintln!("couldn't register perland:// links: {e}");
                }
            }
```

(Task 9 turns the `eprintln!` into `tracing::warn!`.)

- [ ] **Step 4: Verify.** Run `cargo test -p launcher-app && cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.

- [ ] **Step 5: Commit** with message `feat: update busy rule; installers register perland://, AppImages register themselves` and the trailer.

---

### Task 5: Tauri updater wiring, installer config, version 0.2.0

**Prerequisite:** owner step P1 (the public key). If it isn't in the conversation, stop and ask for it. Never ask for the private key.

**Files:**
- Create: `src-tauri/src/updater.rs`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/main.rs`, `src-tauri/src/commands.rs`, `src-tauri/tauri.conf.json`, `Cargo.toml` (workspace version), `Cargo.lock`

**Interfaces:**
- Consumes: `Updates`, `UpdateSource`, `UpdateState`, `START_DELAY` (Task 3); `AppCore::update_blocked` (Task 4); `Channel::effective` (Task 2); `BuildKind::current` (Task 1).
- Produces:
  - Tauri commands `update_status() -> UpdateState`, `update_check() -> UpdateState` and `update_restart() -> ()`;
  - `settings_set`, which now applies the channel;
  - `type Updates = launcher_app::updates::Updates<updater::TauriUpdateSource, host::TauriHost>` in `main.rs`;
  - workspace version `0.2.0`.

- [ ] **Step 1: Dependency.** Add `tauri-plugin-updater` to `src-tauri/Cargo.toml`. Pick the newest 2.x release that is at least 14 days old and has `UpdaterBuilder::restart_after_install` (check with `cargo info tauri-plugin-updater@<version>` or its docs.rs page; 2.14.0 has it but was released on 2026-10-14, so prefer an older one that also has it). Pin it as `"=<version>"` only if the newest is too new; otherwise use `"<major>.<minor>"`.

- [ ] **Step 2: `tauri.conf.json`.**
  - Delete the top-level `"version"` line.
  - Replace `bundle`:

    ```json
    "bundle": {
      "active": true,
      "targets": ["nsis", "appimage", "deb"],
      "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.ico", "icons/icon.png"],
      "windows": {
        "nsis": {
          "installMode": "currentUser",
          "languages": ["English", "Spanish"],
          "displayLanguageSelector": false
        }
      }
    }
    ```

  - Under `plugins`, next to `deep-link`:

    ```json
    "updater": {
      "pubkey": "<the owner's public key, verbatim>",
      "windows": { "installMode": "passive" }
    }
    ```

  - In `Cargo.toml` `[workspace.package]`, set `version = "0.2.0"`, then run `cargo check -p perland-launcher` to refresh `Cargo.lock`.

- [ ] **Step 3: `src-tauri/src/updater.rs`**:

```rust
//! The real `UpdateSource`: Tauri's updater against the channel manifests on GitHub (spec §4.1,
//! §4.7). Every behaviour decision lives in `launcher_app::updates`; this only adapts the plugin.
use std::sync::Arc;

use launcher_app::{
    runner::BoxFuture,
    updates::{Progress, UpdateSource},
};
use launcher_core::{ErrorCode, LauncherError, Result};
use tauri::AppHandle;
use tauri_plugin_updater::{Error, Update, UpdaterExt};

pub struct TauriUpdateSource {
    app: AppHandle,
}

impl TauriUpdateSource {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

/// The plugin's update plus the endpoint it came from (Restart now checks that endpoint again).
pub struct Found {
    update: Update,
    endpoint: &'static str,
}

fn failed(error: Error) -> LauncherError {
    match error {
        // A bad signature or a manifest that isn't ours: never installed.
        Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) | Error::InvalidUpdaterFormat => {
            LauncherError::integrity(
                ErrorCode::LauncherUpdateInvalid,
                "The launcher update didn't pass its safety check, so it wasn't installed",
            )
            .with_detail(error)
        }
        other => LauncherError::network(ErrorCode::LauncherUpdateFailed, "Couldn't check for launcher updates")
            .with_fix("You can keep playing. It tries again next time you open the launcher")
            .with_detail(other),
    }
}

/// Check `endpoint`. `relaunch` is fixed when the updater is built: the plugin passes the NSIS
/// installer `/R` (relaunch) only with `restart_after_install(true)` (spec §4.7).
/// `on_before_exit` is never set, so the plugin's default cleanup still runs before an install.
async fn check_at(app: &AppHandle, endpoint: &'static str, relaunch: bool) -> Result<Option<Update>> {
    let url = endpoint.parse().map_err(|e| {
        LauncherError::network(ErrorCode::LauncherUpdateFailed, "Couldn't check for launcher updates").with_detail(e)
    })?;
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(failed)?
        .restart_after_install(relaunch)
        .build()
        .map_err(failed)?;
    match updater.check().await {
        Ok(found) => Ok(found),
        // No release on this channel yet (stable before Plan 4): nothing newer (Review Focus 1).
        Err(Error::ReleaseNotFound) => Ok(None),
        Err(e) => Err(failed(e)),
    }
}

impl UpdateSource for TauriUpdateSource {
    type Update = Found;

    fn check(&self, endpoint: &'static str) -> BoxFuture<Result<Option<Found>>> {
        let app = self.app.clone();
        Box::pin(async move {
            Ok(check_at(&app, endpoint, false).await?.map(|update| Found { update, endpoint }))
        })
    }

    fn version(&self, found: &Found) -> String {
        found.update.version.clone()
    }

    fn download(&self, found: Arc<Found>, progress: Progress) -> BoxFuture<Result<Vec<u8>>> {
        Box::pin(async move {
            found
                .update
                .download(move |chunk, total| progress(chunk as u64, total), || {})
                .await
                .map_err(failed)
        })
    }

    fn install(&self, found: Arc<Found>, bytes: Arc<Vec<u8>>, relaunch: bool) -> BoxFuture<Result<()>> {
        let app = self.app.clone();
        Box::pin(async move {
            if relaunch && cfg!(windows) {
                // Rebuild with relaunch on and install the bytes already downloaded and verified
                // through it, when it is still the same version. Ends the process.
                if let Ok(Some(fresh)) = check_at(&app, found.endpoint, true).await {
                    if fresh.version == found.update.version {
                        return fresh.install(bytes.as_slice()).map_err(failed);
                    }
                }
            }
            // Windows: starts the installer and ends the process. Linux: replaces the AppImage or
            // installs the .deb (pkexec asks for the password); Linux never relaunches by itself.
            found.update.install(bytes.as_slice()).map_err(failed)?;
            if relaunch && !cfg!(windows) {
                app.restart();
            }
            Ok(())
        })
    }
}
```

Confirm the `Error` variant names (`Minisign`, `Base64`, `SignatureUtf8`, `InvalidUpdaterFormat`, `ReleaseNotFound`) and the `download` closure signature against `tauri-plugin-updater`'s `src/error.rs` and `src/updater.rs` for the pinned version (read the crate under `~/.cargo/registry/src`). Keep the mapping: signature or format problems go to `LauncherUpdateInvalid`, "no release found" goes to `Ok(None)`, and everything else goes to `LauncherUpdateFailed`. If `Update` isn't `Sync`, wrap it as `std::sync::Mutex<Update>` inside `Found`.

- [ ] **Step 4: `main.rs`.**
  - Add `mod updater;` and the alias:

    ```rust
    pub type Updates = launcher_app::updates::Updates<updater::TauriUpdateSource, host::TauriHost>;
    ```

  - Register `.plugin(tauri_plugin_updater::Builder::new().build())` after the opener plugin.
  - Change `build_core` to take `host: Arc<host::TauriHost>` instead of creating it, so the core and the updater share one host. Create the host in `setup` with `let host = Arc::new(host::TauriHost::new(app.handle().clone()));`.
  - After `app.manage(core.clone());`:

    ```rust
            let build = BuildKind::current();
            let settings = tauri::async_runtime::block_on(core.settings_get());
            let updates = Updates::new(
                Arc::new(updater::TauriUpdateSource::new(app.handle().clone())),
                host.clone(),
                build,
                launcher_core::settings::Channel::effective(settings.update_channel, build),
            );
            app.manage(updates.clone());
            tauri::async_runtime::spawn(async move {
                updates.start(launcher_app::updates::START_DELAY).await;
            });
    ```

  - Replace `.run(tauri::generate_context!()).expect(…)` with:

    ```rust
        .build(tauri::generate_context!())
        .expect("Perland Launcher failed to start")
        .run(|app, event| {
            // Quitting installs a ready update, unless something still runs (spec §4.3).
            if let tauri::RunEvent::Exit = event {
                if let (Some(core), Some(updates)) = (app.try_state::<Arc<Core>>(), app.try_state::<Arc<Updates>>()) {
                    let busy = core.update_blocked();
                    tauri::async_runtime::block_on(updates.on_quit(busy));
                }
            }
        });
    ```

  - Add `commands::update_status, commands::update_check, commands::update_restart,` to `generate_handler!`.

- [ ] **Step 5: `commands.rs`.**

```rust
type UpdatesState<'a> = State<'a, Arc<crate::Updates>>;

#[tauri::command]
pub fn update_status(updates: UpdatesState<'_>) -> UpdateState {
    updates.status()
}

#[tauri::command]
pub async fn update_check(updates: UpdatesState<'_>) -> Res<UpdateState> {
    Ok(updates.check().await)
}

#[tauri::command]
pub async fn update_restart(core: CoreState<'_>, updates: UpdatesState<'_>) -> Res<()> {
    updates.restart_now(core.update_blocked()).await
}
```

Change `settings_set` to apply the channel:

```rust
#[tauri::command]
pub async fn settings_set(core: CoreState<'_>, updates: UpdatesState<'_>, settings: Settings) -> Res<Settings> {
    let saved = core.settings_set(settings).await?;
    let channel = Channel::effective(saved.update_channel, BuildKind::current());
    let updates = updates.inner().clone();
    tauri::async_runtime::spawn(async move {
        updates.set_channel(channel).await;
    });
    Ok(saved)
}
```

Import `launcher_app::updates::UpdateState`, `launcher_core::{accounts::BuildKind, settings::Channel}`.

- [ ] **Step 6: Verify.**
  - Run `cargo build -p perland-launcher && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`. Expected: PASS.
  - Run `pnpm tauri dev` with `PERLAND_LAUNCHER_ROOT` set to a scratch folder. The app starts, Settings still works, and the dev build never checks.
  - A local `pnpm tauri build` isn't required. If you run one, it builds NSIS without updater signatures (`createUpdaterArtifacts` is off locally).

- [ ] **Step 7: Commit** with message `feat: Tauri updater wiring, per-user NSIS and Linux bundles, version 0.2.0` and the trailer.

---

### Task 6: Launcher updates in Settings

**Files:**
- Create: `ui/src/features/settings/LauncherUpdates.tsx`, `ui/src/features/settings/LauncherUpdates.test.tsx`
- Modify: `ui/src/ipc/commands.ts`, `ui/src/ipc/mock.ts`, `ui/src/state/queries.ts`, `ui/src/state/handleEvent.ts`, `ui/src/state/handleEvent.test.ts`, `ui/src/features/settings/Settings.tsx`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`

**Interfaces:**
- Consumes: `UpdateState` and the `update` event (Task 3), `settings.updateChannel` (Task 2), the commands `update_status`, `update_check` and `update_restart` (Task 5), `useActiveIds()` from `state/runStore`.
- Produces:
  - `commands.updateStatus()`, `commands.updateCheck()` and `commands.updateRestart()`;
  - `qk.update` and `useUpdate()`;
  - mock option `update?: UpdateState` and the mock handlers;
  - `<LauncherUpdates />`.

- [ ] **Step 1: Write the failing tests.** `LauncherUpdates.test.tsx`:

```tsx
import { screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { renderApp, sent } from '../../test/backend';
import { Settings } from './Settings';

it('shows the state, checks now and switches channel', async () => {
  const { user, calls } = renderApp(<Settings />, { beta: true, update: { state: 'up_to_date' } });
  expect(await screen.findByText(/Up to date · 0\.1\.0/)).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Check now' }));
  expect(sent(calls, 'update_check')).toHaveLength(1);
  const channel = screen.getByRole('combobox', { name: 'Update channel' });
  expect(channel).toHaveValue('beta');
  await user.selectOptions(channel, 'stable');
  expect(await screen.findByText("You'll move to stable once it's newer than your beta.")).toBeInTheDocument();
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { updateChannel: 'stable' } });
});

it('offers Restart now when an update is ready', async () => {
  const { user, calls } = renderApp(<Settings />, { beta: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  expect(await screen.findByText('Version 0.2.0-beta.2 is ready. It installs when you close the launcher.')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Restart now' }));
  expect(sent(calls, 'update_restart')).toHaveLength(1);
});

it('says updates are off in dev builds', async () => {
  renderApp(<Settings />);
  expect(await screen.findByText('Updates are off in developer builds.')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Check now' })).not.toBeInTheDocument();
});

it('shows download progress and a failed check', async () => {
  const { backend } = renderApp(<Settings />, { beta: true, update: { state: 'downloading', percent: 40 } });
  expect(await screen.findByText('Downloading the update… 40%')).toBeInTheDocument();
  backend.emit({ type: 'update', state: { state: 'failed', error: { category: 'network', code: 'launcher_update_failed', message: 'x', fix: null, detail: null, params: {} } } });
  expect(await screen.findByText("Couldn't check for updates.")).toBeInTheDocument();
});
```

The mock app version comes from `contract/app_info.json` (`0.1.0`). If the mock backend's event helper isn't named `emit`, use the name the other tests use for pushing events (`grep -n "emit" ui/src/ipc/mock.ts`). In `handleEvent.test.ts`, add a test that an `update` event lands in `queryClient.getQueryData(qk.update)`.

Run `pnpm --filter ui test -- LauncherUpdates handleEvent`. Expected: FAIL.

- [ ] **Step 2: Plumbing.**
  - **`commands.ts`:**

    ```ts
      updateStatus: () => invoke<UpdateState>('update_status'),
      updateCheck: () => invoke<UpdateState>('update_check'),
      updateRestart: () => invoke<null>('update_restart'),
    ```

  - **`queries.ts`:** `update: ['update'] as const` in `qk`, plus:

    ```ts
    export const useUpdate = () => useQuery({ queryKey: qk.update, queryFn: commands.updateStatus });
    ```

  - **`handleEvent.ts`,** first thing in `handleEvent`:

    ```ts
      if (event.type === 'update') {
        queryClient.setQueryData(qk.update, event.state);
        return;
      }
    ```

  - **`mock.ts`:**
    - Add `update?: UpdateState` to `MockOptions`, and keep `let update: UpdateState = options.update ?? { state: options.release || options.beta ? 'idle' : 'off' };`.
    - `case 'update_status': return update;`
    - `case 'update_check': return update.state === 'off' ? update : (update = update.state === 'ready' ? update : { state: 'up_to_date' });`
    - `case 'update_restart':` reject with `fail('game', 'launcher_update_busy', 'Close your games before restarting to update')` while any mock run is active, else `return null;`. Reuse the mock's own active-run bookkeeping.

- [ ] **Step 3: Component.** `LauncherUpdates.tsx`:

```tsx
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { AppInfo, Channel, UpdateState } from '../../ipc/schema';
import { useUpdate } from '../../state/queries';
import { useActiveIds } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Select } from '../../ui/Select';
import { useSingleFlight } from '../../ui/useSingleFlight';

function statusLine(t: TFunction, state: UpdateState, version: string): string {
  switch (state.state) {
    case 'off':
      return t('settings.updates.off');
    case 'idle':
      return t('settings.updates.idle');
    case 'checking':
      return t('settings.updates.checking');
    case 'downloading':
      return state.percent == null
        ? t('settings.updates.downloading')
        : t('settings.updates.downloadingPercent', { percent: state.percent });
    case 'ready':
      return t('settings.updates.ready', { version: state.version });
    case 'up_to_date':
      return t('settings.updates.upToDate', { version });
    case 'failed':
      return t('settings.updates.failed');
  }
}

/** Settings → Launcher updates (spec §4.5). */
export function LauncherUpdates({
  appInfo,
  choice,
  onChannel,
}: {
  appInfo: AppInfo;
  choice: Channel | null;
  onChannel: (channel: Channel) => void;
}) {
  const { t } = useTranslation();
  const update = useUpdate();
  const busy = useActiveIds().length > 0;
  const once = useSingleFlight();
  const state: UpdateState = update.data ?? { state: appInfo.build === 'dev' ? 'off' : 'idle' };
  const run = (action: () => Promise<unknown>) => once((done) => void action().catch(pushError).finally(done));

  if (appInfo.build === 'dev') {
    return (
      <section className="grid gap-2" aria-labelledby="updates-heading">
        <h2 id="updates-heading" className="font-semibold">{t('settings.updates.title')}</h2>
        <p className="text-sm text-ink-dim">{t('settings.updates.off')}</p>
      </section>
    );
  }
  const channel: Channel = choice ?? (appInfo.build === 'beta' ? 'beta' : 'stable');
  const hint =
    channel === 'stable' && appInfo.build === 'beta'
      ? t('settings.updates.stableFromBeta')
      : t(channel === 'beta' ? 'settings.updates.betaHint' : 'settings.updates.stableHint');
  const working = state.state === 'checking' || state.state === 'downloading';
  return (
    <section className="grid gap-3" aria-labelledby="updates-heading">
      <h2 id="updates-heading" className="font-semibold">{t('settings.updates.title')}</h2>
      <Select
        label={t('settings.updates.channel')}
        value={channel}
        options={[
          { value: 'stable', label: t('settings.updates.stable') },
          { value: 'beta', label: t('settings.updates.beta') },
        ]}
        onChange={(value) => onChannel(value as Channel)}
      />
      <p className="text-sm text-ink-dim">{hint}</p>
      <p role="status" className="text-sm">{statusLine(t, state, appInfo.version)}</p>
      <div className="flex flex-wrap items-center gap-3">
        <Button disabled={working} onClick={() => run(() => commands.updateCheck())}>
          {t('settings.updates.check')}
        </Button>
        {state.state === 'ready' && (
          <Button disabled={busy} onClick={() => run(() => commands.updateRestart())}>
            {t('settings.updates.restart')}
          </Button>
        )}
        {state.state === 'ready' && busy && <p className="text-sm text-ink-dim">{t('settings.updates.busy')}</p>}
      </div>
    </section>
  );
}
```

(Check that `Button` accepts `disabled` and that `Select` passes the label to the combobox's accessible name. `Settings.tsx`'s language select does both.)

In `Settings.tsx`, render it after the data-folder block and before About:

```tsx
        <LauncherUpdates appInfo={appInfo.data} choice={data.updateChannel} onChannel={(updateChannel) => edit({ updateChannel })} />
```

- [ ] **Step 4: Strings.** `en.json` → `settings.updates`:

```json
"updates": {
  "title": "Launcher updates",
  "channel": "Update channel",
  "stable": "Stable",
  "beta": "Beta",
  "stableHint": "Tested releases.",
  "betaHint": "New builds first, with a few rough edges.",
  "stableFromBeta": "You'll move to stable once it's newer than your beta.",
  "off": "Updates are off in developer builds.",
  "idle": "Checks for updates when the launcher opens.",
  "checking": "Checking for updates…",
  "downloading": "Downloading the update…",
  "downloadingPercent": "Downloading the update… {{percent}}%",
  "ready": "Version {{version}} is ready. It installs when you close the launcher.",
  "upToDate": "Up to date · {{version}}",
  "failed": "Couldn't check for updates.",
  "check": "Check now",
  "restart": "Restart now",
  "busy": "Close your games first."
}
```

`es.json` → `settings.updates`:

```json
"updates": {
  "title": "Actualizaciones del launcher",
  "channel": "Canal de actualizaciones",
  "stable": "Estable",
  "beta": "Beta",
  "stableHint": "Versiones probadas.",
  "betaHint": "Lo más nuevo primero, con algún detalle por pulir.",
  "stableFromBeta": "Pasarás a estable cuando sea más nueva que tu beta.",
  "off": "Las actualizaciones están desactivadas en las versiones de desarrollo.",
  "idle": "Busca actualizaciones al abrir el launcher.",
  "checking": "Buscando actualizaciones…",
  "downloading": "Descargando la actualización…",
  "downloadingPercent": "Descargando la actualización… {{percent}} %",
  "ready": "La versión {{version}} está lista. Se instala cuando cierras el launcher.",
  "upToDate": "Al día · {{version}}",
  "failed": "No se pudo buscar actualizaciones.",
  "check": "Buscar ahora",
  "restart": "Reiniciar ahora",
  "busy": "Primero cierra tus juegos."
}
```

- [ ] **Step 5: Verify.** Run `pnpm --filter ui test && pnpm --filter ui typecheck && pnpm --filter ui lint`. Expected: PASS.

- [ ] **Step 6: Commit** with message `feat(ui): Settings → Launcher updates (channel, status, Check now, Restart now)` and the trailer.

---

### Task 7: Update-ready note in the title bar

**Files:**
- Modify: `ui/src/features/titlebar/TitleBar.tsx`, `ui/src/features/titlebar/TitleBar.test.tsx`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`

**Interfaces:**
- Consumes: `useUpdate()`, `commands.updateRestart()` (Task 6), `useActiveIds()`.

- [ ] **Step 1: Write the failing test** in `TitleBar.test.tsx`, following that file's render helper:

```tsx
it('shows a quiet note with Restart now when an update is ready', async () => {
  const { user, calls } = renderApp(<TitleBar />, { beta: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  expect(await screen.findByText('Update ready · installs when you close')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Restart now' }));
  expect(sent(calls, 'update_restart')).toHaveLength(1);
});

it('shows no note otherwise, and none in the minimal bar', async () => {
  renderApp(<TitleBar minimal />, { beta: true, update: { state: 'ready', version: '0.2.0-beta.2' } });
  await screen.findByRole('button', { name: 'Close' });
  expect(screen.queryByText('Update ready · installs when you close')).not.toBeInTheDocument();
});
```

Run `pnpm --filter ui test -- TitleBar`. Expected: FAIL.

- [ ] **Step 2: Implement** inside the `!minimal` fragment, before the account button:

```tsx
            {update.data?.state === 'ready' && (
              <span className="flex items-center gap-2 pr-2 text-sm text-ink-dim">
                {t('titlebar.updateReady')}
                <button
                  type="button"
                  className="tb-btn font-semibold text-ink"
                  disabled={busy}
                  title={busy ? t('settings.updates.busy') : undefined}
                  onClick={() => void commands.updateRestart().catch(pushError)}
                >
                  {t('titlebar.restart')}
                </button>
              </span>
            )}
```

Add `const update = useUpdate();` and `const busy = useActiveIds().length > 0;` at the top. Add strings `titlebar.updateReady`: `"Update ready · installs when you close"` / `"Actualización lista · se instala al cerrar"`, and `titlebar.restart`: `"Restart now"` / `"Reiniciar ahora"`.

- [ ] **Step 3: Verify.** Run `pnpm --filter ui test && pnpm --filter ui typecheck && pnpm --filter ui lint`. Expected: PASS.

- [ ] **Step 4: Commit** with message `feat(ui): quiet update-ready note in the title bar` and the trailer.

---

### Task 8: Launcher log (core)

**Files:**
- Create: `crates/launcher-core/src/logging.rs`
- Modify: `crates/launcher-core/src/lib.rs` (`pub mod logging;`), `crates/launcher-core/Cargo.toml`, `Cargo.toml` (workspace deps)

**Interfaces:**
- Consumes: `launch::Redactor`, `paths::LauncherDirs`, `LauncherError`.
- Produces:
  - `logging::KEEP_FILES = 14`;
  - `logging::dir(&LauncherDirs) -> PathBuf` (`<root>/logs/launcher`);
  - `logging::RedactingFile::new(dir: &Path, redactor: Redactor) -> io::Result<RedactingFile>`, which implements `tracing_subscriber::fmt::MakeWriter`;
  - `logging::init(&LauncherDirs)`, which never fails;
  - `logging::log_error(command: &str, error: &LauncherError)`.

- [ ] **Step 1: Dependencies.** In the workspace `Cargo.toml` add `tracing = "0.1"`, `tracing-subscriber = { version = "0.3", features = ["env-filter"] }` and `tracing-appender = "0.2"`. In `launcher-core` add `tracing.workspace = true`, `tracing-subscriber.workspace = true` and `tracing-appender.workspace = true`.

- [ ] **Step 2: Write the failing tests** (in `logging.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    fn only_file(dir: &Path) -> PathBuf {
        let files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(files.len(), 1, "{files:?}");
        files[0].clone()
    }

    #[test]
    fn the_launcher_log_has_its_own_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = LauncherDirs::new(tmp.path());
        assert_eq!(dir(&dirs), dirs.logs().join("launcher"));
    }

    #[test]
    fn lines_reach_a_daily_file_with_the_home_path_masked() {
        let tmp = tempfile::tempdir().unwrap();
        let file = RedactingFile::new(tmp.path(), Redactor::new().with_home("/home/steve")).unwrap();
        let subscriber = tracing_subscriber::fmt().with_ansi(false).with_writer(file).finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(path = "/home/steve/.minecraft", "opened");
        });
        let log = only_file(tmp.path());
        let name = log.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("launcher.") && name.ends_with(".log"), "{name}");
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("~/.minecraft"), "{text}");
        assert!(!text.contains("/home/steve"), "{text}");
    }

    #[test]
    fn errors_log_their_code_and_detail() {
        let tmp = tempfile::tempdir().unwrap();
        let file = RedactingFile::new(tmp.path(), Redactor::new()).unwrap();
        let subscriber = tracing_subscriber::fmt().with_ansi(false).with_writer(file).finish();
        let error = LauncherError::network(ErrorCode::HostUnreachable, "Couldn't reach example.com").with_detail("dns failed");
        tracing::subscriber::with_default(subscriber, || log_error("play", &error));
        let text = std::fs::read_to_string(only_file(tmp.path())).unwrap();
        assert!(text.contains("WARN"), "{text}");
        assert!(text.contains("command=\"play\""), "{text}");
        assert!(text.contains("HostUnreachable"), "{text}");
        assert!(text.contains("dns failed"), "{text}");
    }
}
```

Run `cargo test -p launcher-core logging::`. Expected: a compile error.

- [ ] **Step 3: Implement** `logging.rs`:

```rust
//! The launcher's own log (spec §5.1): `logs/launcher/launcher.YYYY-MM-DD.log`, a new file each
//! day, the newest 14 kept, the home path masked on every line. Its own folder, so pruning never
//! touches a game's `<id>-latest.log`. Writes are synchronous (no background worker): nothing is
//! lost when an update install ends the process, and the volume is small.
use std::{
    io,
    path::{Path, PathBuf},
};

use tracing_appender::rolling::{RollingFileAppender, RollingWriter, Rotation};
use tracing_subscriber::{fmt::MakeWriter, EnvFilter};

use crate::{error::LauncherError, launch::Redactor, paths::LauncherDirs};

pub const KEEP_FILES: usize = 14;

pub fn dir(dirs: &LauncherDirs) -> PathBuf {
    dirs.logs().join("launcher")
}

/// The rolling log file; every line written through it is redacted first.
pub struct RedactingFile {
    appender: RollingFileAppender,
    redactor: Redactor,
}

impl RedactingFile {
    pub fn new(dir: &Path, redactor: Redactor) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("launcher")
            .filename_suffix("log")
            .max_log_files(KEEP_FILES)
            .build(dir)
            .map_err(io::Error::other)?;
        Ok(Self { appender, redactor })
    }
}

pub struct RedactingWriter<'a> {
    inner: RollingWriter<'a>,
    redactor: &'a Redactor,
}

impl io::Write for RedactingWriter<'_> {
    // The formatter writes one whole event per call, so a home path is never split.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        self.inner.write_all(self.redactor.redact(&text).as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<'a> MakeWriter<'a> for RedactingFile {
    type Writer = RedactingWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter {
            inner: self.appender.make_writer(),
            redactor: &self.redactor,
        }
    }
}

/// Start logging. Never fails the launcher: without a log file it simply runs without one.
/// `PERLAND_LOG` (e.g. `debug`) overrides the default `info` level.
pub fn init(dirs: &LauncherDirs) {
    let Ok(file) = RedactingFile::new(&dir(dirs), Redactor::for_current_user()) else {
        return;
    };
    let filter = EnvFilter::try_from_env("PERLAND_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .with_env_filter(filter)
        .with_writer(file)
        .try_init();
}

/// A command's error, as the player saw it plus its technical detail. Never a token: errors
/// never carry them (spec §5.1), and the home path is masked by the writer.
pub fn log_error(command: &str, error: &LauncherError) {
    tracing::warn!(
        command,
        code = ?error.code,
        detail = error.detail.as_deref().unwrap_or(""),
        "{}",
        error.message
    );
}
```

Add `pub mod logging;` to `lib.rs`. If `RollingFileAppender::make_writer` needs the `MakeWriter` trait in scope for `&RollingFileAppender`, it already is (it's imported).

- [ ] **Step 4: Verify.** Run `cargo test -p launcher-core logging:: && cargo clippy -p launcher-core --all-targets -- -D warnings`. Expected: PASS.

- [ ] **Step 5: Commit** with message `feat(core): launcher log with daily rotation, 14 files kept, home path redacted` and the trailer.

---

### Task 9: Logging wiring

**Files:**
- Create: `crates/launcher-app/src/logged.rs`
- Modify: `crates/launcher-app/src/lib.rs`, `crates/launcher-app/Cargo.toml`, `crates/launcher-app/src/runs.rs:64-65`, `src-tauri/src/main.rs`, `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `logging::{init, log_error}` (Task 8), `AppEvent`, `UpdateState`.
- Produces: `launcher_app::logged::{LoggedHost<H>, log_event(&AppEvent)}`. The `Core` alias becomes `AppCore<CoreRunner, LoggedHost<host::TauriHost>>`.

- [ ] **Step 1: Dependencies.** `launcher-app` gets `tracing.workspace = true`, and `tracing-subscriber.workspace = true` under `[dev-dependencies]`.

- [ ] **Step 2: Write the failing tests** in `logged.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{events::ExitInfo, testing::RecordingHost, updates::UpdateState};
    use launcher_core::{ErrorCode, LauncherError};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> { self.0.lock().unwrap().extend_from_slice(b); Ok(b.len()) }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }

    fn logged(events: Vec<AppEvent>) -> (String, LoggedHost<RecordingHost>) {
        let buf = Buf::default();
        let writer = buf.clone();
        let subscriber = tracing_subscriber::fmt().with_ansi(false).with_writer(move || writer.clone()).finish();
        let host = LoggedHost(RecordingHost::default());
        tracing::subscriber::with_default(subscriber, || {
            for e in events { host.emit(e); }
        });
        let text = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        (text, host)
    }

    #[test]
    fn warnings_runs_and_updates_are_logged_and_passed_on() {
        let (text, host) = logged(vec![
            AppEvent::Warning { instance: None, error: LauncherError::disk(ErrorCode::SettingsDamaged, "Your launcher settings couldn't be read") },
            AppEvent::RunState { instance: "asalith".into(), state: crate::events::RunStateKind::Running, exit: None, update: None },
            AppEvent::RunState { instance: "asalith".into(), state: crate::events::RunStateKind::Idle, exit: Some(ExitInfo { code: Some(1), ..ExitInfo::default() }), update: None },
            AppEvent::Update { state: UpdateState::Ready { version: "0.2.0-beta.2".into() } },
        ]);
        assert!(text.contains("SettingsDamaged"), "{text}");
        assert!(text.contains("game started"), "{text}");
        assert!(text.contains("run ended") && text.contains("Some(1)"), "{text}");
        assert!(text.contains("0.2.0-beta.2"), "{text}");
        assert_eq!(host.0.events.lock().unwrap().len(), 4, "every event still reaches the window");
    }

    #[test]
    fn progress_and_download_ticks_stay_out_of_the_log() {
        let (text, _) = logged(vec![AppEvent::Update { state: UpdateState::Downloading { percent: Some(40) } }]);
        assert!(text.is_empty(), "{text}");
    }
}
```

(`RecordingHost` derives `Default`, so the test owns it directly.) Run `cargo test -p launcher-app logged::`. Expected: a compile error.

- [ ] **Step 3: Implement** `logged.rs`:

```rust
//! What matters from the app's events goes to the launcher log, then on to the window.
use crate::{
    events::{AppEvent, Host, RunStateKind},
    updates::UpdateState,
};

pub struct LoggedHost<H: Host>(pub H);

impl<H: Host> Host for LoggedHost<H> {
    fn emit(&self, event: AppEvent) {
        log_event(&event);
        self.0.emit(event);
    }
    fn minimize(&self) {
        self.0.minimize()
    }
    fn restore(&self) {
        self.0.restore()
    }
}

pub fn log_event(event: &AppEvent) {
    match event {
        AppEvent::Warning { instance, error } => tracing::warn!(
            instance = instance.as_deref().unwrap_or(""),
            code = ?error.code,
            detail = error.detail.as_deref().unwrap_or(""),
            "{}",
            error.message
        ),
        AppEvent::RunState { instance, state: RunStateKind::Running, .. } => {
            tracing::info!(instance = instance.as_str(), "game started")
        }
        AppEvent::RunState { instance, exit: Some(exit), .. } => tracing::info!(
            instance = instance.as_str(),
            code = ?exit.code,
            killed = exit.killed,
            cancelled = exit.cancelled,
            crashed = exit.is_crash(),
            error = ?exit.error.as_ref().map(|e| e.code),
            synced = ?exit.synced.as_ref().map(|s| (s.added, s.updated, s.removed)),
            "run ended"
        ),
        AppEvent::Update { state: UpdateState::Downloading { .. } } => {}
        AppEvent::Update { state } => tracing::info!(state = ?state, "launcher update"),
        _ => {}
    }
}
```

Add `pub mod logged;` to `lib.rs`. Run `cargo test -p launcher-app logged::`: PASS.

- [ ] **Step 4: Log the play.** In `runs.rs` `play_with`, right after `let instance = self.instances.load(id).await?;`:

```rust
        tracing::info!(
            instance = id,
            minecraft = instance.game.minecraft.as_str(),
            loader = ?instance.game.loader.as_ref().map(|l| (l.kind, l.version.as_str())),
            join,
            "play"
        );
```

- [ ] **Step 5: Wire it in `src-tauri`.**
  - **`main.rs`:**
    - First line of `main()`: compute `root` the same way `build_core` does (move that logic into `fn data_root() -> PathBuf`, falling back to `"."` only if neither is set), then call `launcher_core::logging::init(&LauncherDirs::new(root))`.
    - Change the aliases to `pub type Host = launcher_app::logged::LoggedHost<host::TauriHost>;`, `pub type Core = AppCore<CoreRunner, Host>;` and `pub type Updates = launcher_app::updates::Updates<updater::TauriUpdateSource, Host>;`, create the host as `Arc::new(LoggedHost(host::TauriHost::new(app.handle().clone())))`, and pass that one `Arc` to both.
    - Log the startup in `setup`:

      ```rust
      tracing::info!(version, build = ?build, channel = ?channel, os = std::env::consts::OS, arch = std::env::consts::ARCH, "Perland Launcher started")
      ```

    - Change Task 4's `eprintln!` to `tracing::warn!(error = %e, "couldn't register perland:// links")`.
  - **`commands.rs`:** add

    ```rust
    /// Every error a command returns goes to the launcher log.
    fn logged<T>(command: &'static str, result: Res<T>) -> Res<T> {
        if let Err(error) = &result {
            launcher_core::logging::log_error(command, error);
        }
        result
    }
    ```

    Wrap each command's returned `Res` with it, for example `logged("play", core.play_with(&id, join).await)`. This is mechanical across the file. Don't wrap `update_status` (it returns no `Res`).

- [ ] **Step 6: Verify.**
  - Run `cargo build -p perland-launcher && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`. Expected: PASS.
  - Run `pnpm tauri dev` with a scratch `PERLAND_LAUNCHER_ROOT`, and confirm `<root>/logs/launcher/launcher.<today>.log` has the startup line with no home path.

- [ ] **Step 7: Commit** with message `feat: launcher log wiring (startup, runs, updates, command errors)` and the trailer.

---

### Task 10: Diagnostics export (core)

**Files:**
- Create: `crates/launcher-core/src/diagnostics.rs`
- Modify: `crates/launcher-core/src/lib.rs`

**Interfaces:**
- Consumes: `LauncherDirs`, `Redactor`, `invite::InviteLink::{parse, token}`, `download::redact_url`, `logging::dir` (Task 8).
- Produces:
  - `diagnostics::SystemInfo { launcher_version, build, update_channel, os, arch, total_memory_mb }` (Serialize, camelCase);
  - `diagnostics::LOG_TAIL_BYTES = 5 MiB` and `Limits { log_tail: u64 }` (Default);
  - `async export(&LauncherDirs, &SystemInfo, &Path) -> Result<()>`;
  - `async export_with(&LauncherDirs, &SystemInfo, &Path, Redactor, Limits) -> Result<()>`.

- [ ] **Step 1: Write the failing tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    const HOME: &str = "C:\\Users\\steve";

    fn token() -> String {
        "A".repeat(43)
    }

    fn info() -> SystemInfo {
        SystemInfo {
            launcher_version: "0.2.0-beta.1".into(),
            build: "beta".into(),
            update_channel: "beta".into(),
            os: "Windows 11 Pro".into(),
            arch: "x86_64".into(),
            total_memory_mb: 16384,
        }
    }

    /// A data folder with one Perland server instance, logs, settings and accounts.
    fn seed() -> (tempfile::TempDir, LauncherDirs) {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = LauncherDirs::new(tmp.path().join("data"));
        let inst = dirs.instances().join("asalith");
        std::fs::create_dir_all(inst.join(".perland")).unwrap();
        std::fs::write(
            inst.join("instance.json"),
            format!(r#"{{"id":"asalith","name":"Asalith","source":{{"kind":"perland","invite":"https://play.example.com/{}","engine":"https://engine.example.com"}}}}"#, token()),
        )
        .unwrap();
        std::fs::write(inst.join(".perland/managed.json"), r#"{"files":[]}"#).unwrap();
        let broken = dirs.instances().join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("instance.json"), format!("{{ not json {HOME} {}", token())).unwrap();
        std::fs::create_dir_all(crate::logging::dir(&dirs)).unwrap();
        std::fs::write(crate::logging::dir(&dirs).join("launcher.2026-10-14.log"), format!("opened {HOME}\\AppData\njoin https://play.example.com/{}\n", token())).unwrap();
        std::fs::write(dirs.logs().join("asalith-latest.log"), format!("[main] game dir {HOME}\\x\n")).unwrap();
        std::fs::write(dirs.root().join("settings.json"), r#"{"language":"es"}"#).unwrap();
        std::fs::write(dirs.root().join("accounts.json"), r#"{"accounts":[]}"#).unwrap();
        std::fs::create_dir_all(dirs.java().join("java-runtime-delta")).unwrap();
        (tmp, dirs)
    }

    fn entries(zip: &Path) -> Vec<(String, String)> {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        (0..archive.len())
            .map(|i| {
                let mut f = archive.by_index(i).unwrap();
                let mut text = String::new();
                f.read_to_string(&mut text).unwrap();
                (f.name().to_string(), text)
            })
            .collect()
    }

    async fn run(dirs: &LauncherDirs, out: &Path, limits: Limits) -> Result<()> {
        export_with(dirs, &info(), out, Redactor::new().with_home(HOME), limits).await
    }

    #[tokio::test]
    async fn the_zip_holds_what_support_needs_and_never_accounts() {
        let (tmp, dirs) = seed();
        let out = tmp.path().join("perland-diagnostics.zip");
        run(&dirs, &out, Limits::default()).await.unwrap();
        let mut names: Vec<_> = entries(&out).into_iter().map(|(n, _)| n).collect();
        names.sort();
        assert_eq!(names, vec![
            "instances/asalith/instance.json",
            "instances/asalith/managed.json",
            "instances/broken/instance.json",
            "logs/asalith-latest.log",
            "logs/launcher/launcher.2026-10-14.log",
            "settings.json",
            "system.json",
        ]);
        let system = entries(&out).into_iter().find(|(n, _)| n == "system.json").unwrap().1;
        assert!(system.contains("\"build\": \"beta\""), "{system}");
        assert!(system.contains("java-runtime-delta"), "{system}");
        assert!(system.contains("\"instances\": 2"), "{system}");
    }

    #[tokio::test]
    async fn tokens_and_the_home_path_are_redacted_everywhere() {
        let (tmp, dirs) = seed();
        let out = tmp.path().join("d.zip");
        run(&dirs, &out, Limits::default()).await.unwrap();
        for (name, text) in entries(&out) {
            assert!(!text.contains(&token()), "{name} leaks the invite token:\n{text}");
            assert!(!text.contains(HOME), "{name} leaks the home path:\n{text}");
        }
    }

    #[tokio::test]
    async fn a_damaged_instance_file_is_left_out() {
        let (tmp, dirs) = seed();
        let out = tmp.path().join("d.zip");
        run(&dirs, &out, Limits::default()).await.unwrap();
        let broken = entries(&out).into_iter().find(|(n, _)| n == "instances/broken/instance.json").unwrap().1;
        assert_eq!(broken.trim(), "\"instance.json couldn't be read\"");
    }

    #[tokio::test]
    async fn long_logs_keep_their_tail() {
        let (tmp, dirs) = seed();
        let lines: String = (0..20).map(|i| format!("line {i:02}\n")).collect();
        std::fs::write(dirs.logs().join("asalith-latest.log"), &lines).unwrap();
        let out = tmp.path().join("d.zip");
        run(&dirs, &out, Limits { log_tail: 40 }).await.unwrap();
        let log = entries(&out).into_iter().find(|(n, _)| n == "logs/asalith-latest.log").unwrap().1;
        assert!(log.starts_with("[earlier lines cut]\n"), "{log}");
        assert!(log.ends_with("line 19\n"), "{log}");
        assert!(!log.contains("line 00"), "{log}");
    }

    #[tokio::test]
    async fn missing_folder_fails_cleanly() {
        let (tmp, dirs) = seed();
        let out = tmp.path().join("nope").join("d.zip");
        let err = run(&dirs, &out, Limits::default()).await.unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::DiskWriteFailed);
        assert!(!tmp.path().join("nope").exists());
    }

    #[tokio::test]
    async fn an_existing_file_is_replaced_and_no_part_file_is_left() {
        let (tmp, dirs) = seed();
        let out = tmp.path().join("d.zip");
        std::fs::write(&out, "old").unwrap();
        run(&dirs, &out, Limits::default()).await.unwrap();
        assert!(!entries(&out).is_empty());
        assert!(!tmp.path().join("d.zip.part").exists());
    }
}
```

(Check the `SourceRef::Perland` JSON shape in `instances.rs`. If its tag isn't `"kind": "perland"`, change the seed to match a real `instance.json`. The redaction must work on the real shape.)

Run `cargo test -p launcher-core diagnostics::`. Expected: a compile error.

- [ ] **Step 2: Implement** `diagnostics.rs`:

```rust
//! Export diagnostics (spec §5.2): one zip with what someone helping a player needs (logs,
//! instance files, settings, system info), with the home path and every invite token masked.
//! Never `accounts.json`, `jvm-args.txt`, caches or game files.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use serde::Serialize;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use crate::{
    download::redact_url,
    error::{ErrorCode, LauncherError, Result},
    invite::InviteLink,
    launch::Redactor,
    paths::LauncherDirs,
};

pub const LOG_TAIL_BYTES: u64 = 5 << 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub launcher_version: String,
    pub build: String,
    pub update_channel: String,
    pub os: String,
    pub arch: String,
    pub total_memory_mb: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Each log is cut to its last this-many bytes.
    pub log_tail: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { log_tail: LOG_TAIL_BYTES }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemJson<'a> {
    #[serde(flatten)]
    info: &'a SystemInfo,
    java_runtimes: Vec<String>,
    instances: usize,
}

pub async fn export(dirs: &LauncherDirs, info: &SystemInfo, out: &Path) -> Result<()> {
    export_with(dirs, info, out, Redactor::for_current_user(), Limits::default()).await
}

pub async fn export_with(
    dirs: &LauncherDirs,
    info: &SystemInfo,
    out: &Path,
    redactor: Redactor,
    limits: Limits,
) -> Result<()> {
    let (dirs, info, out) = (dirs.clone(), info.clone(), out.to_path_buf());
    tokio::task::spawn_blocking(move || write_zip(&dirs, &info, &out, redactor, limits))
        .await
        .map_err(|e| LauncherError::from(std::io::Error::other(e)))?
}

fn write_failed(e: impl std::fmt::Display) -> LauncherError {
    LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save the diagnostics file")
        .with_param("item", "diagnostics")
        .with_fix("Pick another folder, and check there's free disk space")
        .with_detail(e)
}

/// Directory entries (name, path) sorted by name; a missing folder is empty.
fn listing(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    out.sort();
    out
}

/// `instance.json` with its invite masked and any pack URL stripped of query and user info,
/// plus the tokens found so they're masked everywhere else. A file that doesn't parse is
/// replaced by a note: its raw contents could hold anything.
fn sanitize_instance(raw: &str) -> (String, Vec<String>) {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return ("\"instance.json couldn't be read\"\n".into(), Vec::new());
    };
    let mut tokens = Vec::new();
    if let Some(source) = value.get_mut("source").and_then(|s| s.as_object_mut()) {
        if let Some(invite) = source.get("invite").and_then(|v| v.as_str()).map(str::to_owned) {
            if let Ok(link) = InviteLink::parse(&invite) {
                tokens.push(link.token().to_string());
            }
            source.insert("invite".into(), "<redacted>".into());
        }
        if let Some(url) = source.get("url").and_then(|v| v.as_str()).map(str::to_owned) {
            source.insert("url".into(), redact_url(&url).into());
        }
    }
    (serde_json::to_string_pretty(&value).unwrap_or_default() + "\n", tokens)
}

/// The last `limit` bytes, starting at a whole line.
fn tail(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let mut bytes = Vec::new();
    if len <= limit {
        file.read_to_end(&mut bytes)?;
        return Ok(bytes);
    }
    file.seek(SeekFrom::Start(len - limit))?;
    file.take(limit).read_to_end(&mut bytes)?;
    let start = bytes.iter().position(|&b| b == b'\n').map_or(bytes.len(), |i| i + 1);
    let mut out = b"[earlier lines cut]\n".to_vec();
    out.extend_from_slice(&bytes[start..]);
    Ok(out)
}

fn write_zip(dirs: &LauncherDirs, info: &SystemInfo, out: &Path, mut redactor: Redactor, limits: Limits) -> Result<()> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let instances: Vec<_> = listing(&dirs.instances()).into_iter().filter(|(_, p)| p.is_dir()).collect();
    for (id, dir) in &instances {
        if let Ok(raw) = std::fs::read_to_string(dir.join("instance.json")) {
            let (text, tokens) = sanitize_instance(&raw);
            for token in tokens {
                redactor = redactor.with_secret(token);
            }
            entries.push((format!("instances/{id}/instance.json"), text.into_bytes()));
        }
        if let Ok(bytes) = std::fs::read(dir.join(".perland").join("managed.json")) {
            entries.push((format!("instances/{id}/managed.json"), bytes));
        }
    }
    if let Ok(bytes) = std::fs::read(dirs.root().join("settings.json")) {
        entries.push(("settings.json".into(), bytes));
    }
    for (name, path) in listing(&crate::logging::dir(dirs)) {
        if name.ends_with(".log") {
            if let Ok(bytes) = tail(&path, limits.log_tail) {
                entries.push((format!("logs/launcher/{name}"), bytes));
            }
        }
    }
    for (name, path) in listing(&dirs.logs()) {
        if name.ends_with("-latest.log") && path.is_file() {
            if let Ok(bytes) = tail(&path, limits.log_tail) {
                entries.push((format!("logs/{name}"), bytes));
            }
        }
    }
    let system = SystemJson {
        info,
        java_runtimes: listing(&dirs.java()).into_iter().filter(|(_, p)| p.is_dir()).map(|(n, _)| n).collect(),
        instances: instances.len(),
    };
    entries.push(("system.json".into(), serde_json::to_vec_pretty(&system).unwrap_or_default()));

    let part = out.with_extension("zip.part");
    let written = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut zip = ZipWriter::new(File::create(&part)?);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in entries {
            zip.start_file(name, options)?;
            zip.write_all(redactor.redact(&String::from_utf8_lossy(&bytes)).as_bytes())?;
        }
        zip.finish()?;
        std::fs::rename(&part, out)?;
        Ok(())
    })();
    written.map_err(|e| {
        let _ = std::fs::remove_file(&part);
        write_failed(e)
    })
}
```

Add `pub mod diagnostics;` to `lib.rs`. If `Redactor::with_home` doesn't handle Windows backslashes on Linux CI, the test still passes, because it uses the same literal on both sides. Run `cargo test -p launcher-core diagnostics::`: PASS.

- [ ] **Step 3: Clippy.** Run `cargo clippy -p launcher-core --all-targets -- -D warnings`. Expected: PASS.

- [ ] **Step 4: Commit** with message `feat(core): export diagnostics zip with tokens and home path redacted` and the trailer.

---

### Task 11: Export diagnostics in the app

**Files:**
- Modify: `crates/launcher-app/src/app.rs`, `crates/launcher-core/src/error.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`, `src-tauri/src/host.rs`, `src-tauri/capabilities/default.json`
- Create: `ui/src/features/settings/Diagnostics.tsx`, `ui/src/features/settings/Diagnostics.test.tsx`
- Modify: `ui/src/ipc/commands.ts`, `ui/src/ipc/mock.ts`, `ui/src/features/settings/Settings.tsx`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Regenerate: `ui/src/ipc/contract/error_codes.json`

**Interfaces:**
- Consumes: `diagnostics::{export, SystemInfo}` (Task 10), `Channel::effective` (Task 2).
- Produces:
  - `ErrorCode::DiagnosticsPathInvalid`;
  - `AppCore::diagnostics_export(&self, out: PathBuf, os: String) -> Result<PathBuf>` and `AppCore::diagnostics_last(&self) -> Option<PathBuf>`;
  - commands `diagnostics_export(path) -> String` and `diagnostics_reveal() -> ()`;
  - `host::os_description() -> String`;
  - UI `commands.diagnosticsExport(path)`, `commands.diagnosticsReveal()` and `<Diagnostics />`.

- [ ] **Step 1: Write the failing core tests.** In `error.rs` add `(ErrorCode::DiagnosticsPathInvalid, "diagnostics_path_invalid")` to a new `diagnostics_code_exists` test, shaped like `launcher_update_codes_exist`. In `app.rs` tests:

```rust
#[tokio::test]
async fn diagnostics_export_writes_a_zip_and_remembers_it() {
    let (core, _, _, tmp) = core(BuildKind::Beta);
    let out = tmp.path().join("perland-diagnostics.zip");
    assert_eq!(core.diagnostics_export(out.clone(), "Windows 11".into()).await.unwrap(), out);
    assert!(out.is_file());
    assert_eq!(core.diagnostics_last(), Some(out));
}

#[tokio::test]
async fn diagnostics_must_be_a_zip() {
    let (core, _, _, tmp) = core(BuildKind::Beta);
    let err = core.diagnostics_export(tmp.path().join("notes.txt"), "Windows 11".into()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::DiagnosticsPathInvalid);
    assert_eq!(core.diagnostics_last(), None);
}
```

Run `cargo test -p launcher-app diagnostics`. Expected: a compile error.

- [ ] **Step 2: Implement.**
  - Add `DiagnosticsPathInvalid,` to `error_codes!` under `// settings and logs`.
  - In `AppCore`, add the field `last_export: Mutex<Option<PathBuf>>` (initialised with `Mutex::new(None)` in `new`), and:

```rust
    /// Export diagnostics to `out` (a `.zip` the player picked in the save dialog).
    pub async fn diagnostics_export(&self, out: PathBuf, os: String) -> Result<PathBuf> {
        if !out.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
            return Err(LauncherError::disk(
                ErrorCode::DiagnosticsPathInvalid,
                "Diagnostics are saved as a .zip file",
            )
            .with_fix("Pick a file name that ends in .zip"));
        }
        let channel = Channel::effective(self.settings_get().await.update_channel, self.config.build);
        let info = SystemInfo {
            launcher_version: self.config.launcher_version.clone(),
            build: format!("{:?}", self.config.build).to_lowercase(),
            update_channel: format!("{channel:?}").to_lowercase(),
            os,
            arch: std::env::consts::ARCH.into(),
            total_memory_mb: self.config.system_memory_mb,
        };
        launcher_core::diagnostics::export(&self.config.dirs, &info, &out).await?;
        *self.last_export.lock().unwrap_or_else(|e| e.into_inner()) = Some(out.clone());
        Ok(out)
    }

    /// The file the last export wrote, for "Show in folder" (never a path from the webview).
    pub fn diagnostics_last(&self) -> Option<PathBuf> {
        self.last_export.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
```

  - Add the i18n errors:
    - en: `"diagnostics_path_invalid": { "message": "Diagnostics are saved as a .zip file.", "fix": "Pick a file name that ends in .zip." }`
    - es: `"diagnostics_path_invalid": { "message": "El diagnóstico se guarda como archivo .zip.", "fix": "Elige un nombre que termine en .zip." }`
  - Run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, then `cargo test -p launcher-app`. Expected: PASS.

- [ ] **Step 3: Wire it in Tauri.**
  - **`host.rs`:**

    ```rust
    /// "Windows 11 Pro 26200" or "Linux 24.04 Ubuntu", for diagnostics.
    pub fn os_description() -> String {
        sysinfo::System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string())
    }
    ```

  - **`commands.rs`:**

    ```rust
    #[tauri::command]
    pub async fn diagnostics_export(core: CoreState<'_>, path: PathBuf) -> Res<String> {
        logged("diagnostics_export", core.diagnostics_export(path, crate::host::os_description()).await)
            .map(|p| p.display().to_string())
    }

    #[tauri::command]
    pub fn diagnostics_reveal(app: AppHandle, core: CoreState<'_>) -> Res<()> {
        if let Some(path) = core.diagnostics_last() {
            let _ = app.opener().reveal_item_in_dir(path);
        }
        Ok(())
    }
    ```

  - Register both in `main.rs`, and add `"dialog:allow-save"` to `capabilities/default.json`.

- [ ] **Step 4: Write the failing UI test** in `Diagnostics.test.tsx`:

```tsx
import { screen, waitFor } from '@testing-library/react';
import { expect, it } from 'vitest';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { Settings } from './Settings';

it('saves diagnostics where the player picks and offers Show in folder', async () => {
  const { user, calls } = renderApp(<Settings />, { beta: true });
  await user.click(await screen.findByRole('button', { name: 'Export diagnostics' }));
  await waitFor(() => expect(sent(calls, 'diagnostics_export')).toHaveLength(1));
  expect(sent(calls, 'diagnostics_export')[0]).toMatchObject({ path: expect.stringMatching(/\.zip$/) });
  await waitFor(() => expect(toastsSnapshot().some((t) => t.notice?.title === 'Diagnostics saved')).toBe(true));
  toastsSnapshot().find((t) => t.notice?.title === 'Diagnostics saved')!.notice!.action!.run();
  expect(sent(calls, 'diagnostics_reveal')).toHaveLength(1);
});

it('does nothing when the save dialog is cancelled', async () => {
  const { user, calls } = renderApp(<Settings />, { beta: true, savePath: null });
  await user.click(await screen.findByRole('button', { name: 'Export diagnostics' }));
  await new Promise((r) => setTimeout(r, 20));
  expect(sent(calls, 'diagnostics_export')).toHaveLength(0);
});
```

Run `pnpm --filter ui test -- Diagnostics`. Expected: FAIL.

- [ ] **Step 5: Implement the UI.**
  - **`commands.ts`:**

    ```ts
      diagnosticsExport: (path: string) => invoke<string>('diagnostics_export', { path }),
      diagnosticsReveal: () => invoke<null>('diagnostics_reveal'),
    ```

  - **`mock.ts`:**
    - Add `savePath?: string | null` to `MockOptions`.
    - `case 'plugin:dialog|save': return options.savePath === undefined ? 'C:/Users/player/Downloads/perland-diagnostics.zip' : options.savePath;`
    - `case 'diagnostics_export': return (args as { path: string }).path;`
    - `case 'diagnostics_reveal': return null;`
    - Also mock `plugin:path|resolve_directory` (used by `downloadDir()`) to return `'C:/Users/player/Downloads'` if it isn't already handled.
  - **`Diagnostics.tsx`:**

```tsx
import { downloadDir, join } from '@tauri-apps/api/path';
import { save } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { pushError, pushNotice } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { useSingleFlight } from '../../ui/useSingleFlight';

const today = () => new Date().toISOString().slice(0, 10);

/** Settings → Diagnostics (spec §5.2): a zip for whoever helps the player; nothing uploads. */
export function Diagnostics() {
  const { t } = useTranslation();
  const once = useSingleFlight();
  const exportZip = async () => {
    const name = `perland-diagnostics-${today()}.zip`;
    const defaultPath = await downloadDir().then((dir) => join(dir, name)).catch(() => name);
    const path = await save({ defaultPath, filters: [{ name: 'Zip', extensions: ['zip'] }] });
    if (!path) return;
    await commands.diagnosticsExport(path);
    pushNotice({
      title: t('settings.diagnostics.saved'),
      action: { label: t('settings.diagnostics.show'), run: () => void commands.diagnosticsReveal().catch(pushError) },
    });
  };
  return (
    <section className="grid gap-2" aria-labelledby="diagnostics-heading">
      <h2 id="diagnostics-heading" className="font-semibold">{t('settings.diagnostics.title')}</h2>
      <p className="text-sm text-ink-dim">{t('settings.diagnostics.body')}</p>
      <div>
        <Button onClick={() => once((done) => void exportZip().catch(pushError).finally(done))}>
          {t('settings.diagnostics.export')}
        </Button>
      </div>
    </section>
  );
}
```

  - Render `<Diagnostics />` after `<LauncherUpdates />` in `Settings.tsx`.
  - **Strings:**
    - en `settings.diagnostics`: `{ "title": "Diagnostics", "body": "Saves your logs and instance settings in a .zip for whoever helps you. Your accounts and the names of your personal folders stay out.", "export": "Export diagnostics", "saved": "Diagnostics saved", "show": "Show in folder" }`
    - es: `{ "title": "Diagnóstico", "body": "Guarda tus registros y la configuración de tus instancias en un .zip para quien te ayude. Tus cuentas y los nombres de tus carpetas personales quedan fuera.", "export": "Exportar diagnóstico", "saved": "Diagnóstico guardado", "show": "Mostrar en la carpeta" }`

- [ ] **Step 6: Verify.** Run `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm --filter ui test && pnpm --filter ui typecheck && pnpm --filter ui lint`. Expected: PASS. Then export once from `pnpm tauri dev` and open the zip.

- [ ] **Step 7: Commit** with message `feat: Export diagnostics in Settings (save dialog, Show in folder)` and the trailer.

---

### Task 12: `/impeccable` pass on the new surfaces

**Files:** `ui/src/features/settings/{LauncherUpdates,Diagnostics,Settings}.tsx`, `ui/src/features/titlebar/TitleBar.tsx`, and their tests if the copy or roles change.

This task is run by the controller (not a fresh subagent): invoke the `/impeccable` skill, scoped to Settings → Launcher updates, Settings → Diagnostics and the title-bar update note. Use `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md` as the contract.

- [ ] **Step 1: Run `/impeccable`** with this brief:
  - **What to polish:** the Settings "Launcher updates" and "Diagnostics" sections, and the title-bar update-ready note.
  - **Constraints:**
    - one lit button per view (About's buttons and these must not compete);
    - lime only for selection, blue for progress (the download percentage);
    - the note stays quiet and is never the lit button;
    - en/es copy lengths both fit at 960 px wide.
- [ ] **Step 2: Apply its fixes, keeping the tests' accessible names** (or update the tests to the new copy in both languages).
- [ ] **Step 3: Check the result in the browser.** Run `pnpm --filter ui dev:mock` and look at Settings in en and es, and with each update state (mock options). Screenshot it for the PR.
- [ ] **Step 4: Verify.** Run `pnpm --filter ui test && pnpm --filter ui typecheck && pnpm --filter ui lint`. Expected: PASS.
- [ ] **Step 5: Commit** with message `style(ui): impeccable pass on launcher updates, diagnostics and the update note` and the trailer.

---

### Task 13: Body-size caps on metadata fetches

**Files:**
- Modify: `crates/launcher-core/src/http.rs` (shared `read_capped`, its tests)
- Modify: `crates/launcher-core/src/invite/client.rs:194-210` (use the shared one)
- Modify: `crates/launcher-core/src/meta/mod.rs:60-120` (cap, `with_limit`)
- Modify: `crates/launcher-core/src/sources/modrinth.rs:131`, `crates/launcher-core/src/sources/packwiz.rs:385`
- Test: `crates/launcher-core/tests/` (wiremock; follow the existing meta test file's pattern)

**Interfaces:**
- Produces:
  - `http::read_capped(response: reqwest::Response, cap: usize) -> Result<Option<Vec<u8>>, reqwest::Error>` (`None` when larger than `cap`);
  - `meta::MAX_META_BYTES = 32 MiB` and `MetaClient::with_limit(self, usize) -> Self`;
  - `sources::modrinth::MAX_API_BYTES = 8 MiB`;
  - `sources::packwiz::MAX_PACK_TOML_BYTES = 1 MiB`.

- [ ] **Step 1: Move `read_capped`.** Move the function verbatim from `invite/client.rs` to `http.rs` as `pub async fn read_capped`, and change `invite/client.rs` to `use crate::http::read_capped;`. Move its unit tests too, if any. Run `cargo test -p launcher-core invite`. Expected: PASS (a pure move).

- [ ] **Step 2: Write the failing tests** (wiremock, tiny bodies):

```rust
#[tokio::test]
async fn meta_refuses_an_answer_over_its_cap() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::path("/v.json"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("x".repeat(64)))
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(launcher_core::http::client("test").unwrap(), tmp.path().to_path_buf()).with_limit(16);
    let err = meta.get_bytes(&format!("{}/v.json", server.uri())).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);
}

#[tokio::test]
async fn meta_accepts_an_answer_at_its_cap() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::path("/v.json"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("y".repeat(16)))
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(launcher_core::http::client("test").unwrap(), tmp.path().to_path_buf()).with_limit(16);
    let body = meta.get_bytes(&format!("{}/v.json", server.uri())).await.unwrap();
    assert_eq!(body.len(), 16);
}

#[tokio::test]
async fn read_capped_cuts_off_at_the_cap() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::path("/b"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("z".repeat(64)))
        .mount(&server)
        .await;
    let http = launcher_core::http::client("test").unwrap();
    let url = format!("{}/b", server.uri());
    let over = launcher_core::http::read_capped(http.get(&url).send().await.unwrap(), 16).await.unwrap();
    assert_eq!(over, None);
    let fits = launcher_core::http::read_capped(http.get(&url).send().await.unwrap(), 64).await.unwrap();
    assert_eq!(fits.map(|b| b.len()), Some(64));
}
```

`MetaClient::new(http, dir)` and `get_bytes(&str)` are the existing names (`meta/mod.rs`; `packwiz.rs` calls `src.meta.get_bytes`).

Run `cargo test -p launcher-core cap`. Expected: FAIL.

- [ ] **Step 3: Implement.**
  - **`meta/mod.rs`:**
    - Add the constant `pub const MAX_META_BYTES: usize = 32 << 20;`.
    - Add a `limit: usize` field (default `MAX_META_BYTES`) and `pub fn with_limit(mut self, limit: usize) -> Self`.
    - In `fetch`, replace `resp.bytes().await…` with:

      ```rust
                let bytes = match crate::http::read_capped(resp, self.limit).await {
                    Ok(Some(bytes)) => bytes,
                    Ok(None) => {
                        let too_big = LauncherError::loader(ErrorCode::MetaUnexpected, format!("Unexpected data from {}", host_of(url)))
                            .with_param("host", host_of(url))
                            .with_detail(format!("the answer is over {} MB", self.limit >> 20));
                        return match cached {
                            Some(c) if validate(&c).is_ok() => Ok(c),
                            _ => Err(too_big),
                        };
                    }
                    Err(e) => {
                        return Err(LauncherError::network(
                            ErrorCode::ConnectionDropped,
                            format!("Downloading from {} was interrupted", host_of(url)),
                        )
                        .with_param("host", host_of(url))
                        .with_detail(e.without_url()))
                    }
                };
      ```

      Adapt the rest of `fetch` from `bytes` (a `Bytes`) to `Vec<u8>`: `validate(&bytes)`, `write_atomic(&body_path, &bytes)`, `Ok(bytes)`.
  - **`modrinth.rs`:** replace `response.json().await…` with `read_capped(response, MAX_API_BYTES)`. `None` gives the same `MetaUnexpected` error with detail `"the answer is too big"`, and `Some` goes through `serde_json::from_slice`, mapping its error the same way the old `.json()` error was mapped.
  - **`packwiz.rs` `check`:** `match crate::http::read_capped(response, MAX_PACK_TOML_BYTES).await { Ok(Some(b)) => b, Ok(None) => return unavailable("pack.toml is over 1 MB".into()), Err(e) => return unavailable(e.without_url().to_string()) }`.

- [ ] **Step 4: Verify.** Run `cargo test -p launcher-core && cargo clippy -p launcher-core --all-targets -- -D warnings`. Expected: PASS.

- [ ] **Step 5: Commit** with message `fix(core): cap metadata, Modrinth API and pack.toml answers` and the trailer.

---

### Task 14: Windows argfile code page

**Files:**
- Modify: `crates/launcher-core/src/launch/mod.rs:325-366` (`encode_argfile`, `with_argfile`, tests)
- Modify: `crates/launcher-core/Cargo.toml`

**Interfaces:**
- Produces: `launch::encode_argfile(text: &str) -> Option<Vec<u8>>`. It returns the ANSI code page bytes on Windows (UTF-8 when the code page is 65001) or UTF-8 elsewhere, and `None` when a character has no form in that code page. `with_argfile` returns the command unchanged when it gets `None`.

- [ ] **Step 1: Dependency.** Add this to `crates/launcher-core/Cargo.toml` (0.61 is already in `Cargo.lock`):

```toml
[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61", features = ["Win32_Globalization"] }
```

- [ ] **Step 2: Write the failing tests** (in `launch/mod.rs` tests):

```rust
#[test]
fn ascii_argfiles_encode_as_is() {
    assert_eq!(encode_argfile("\"-Xmx4G\"\n").unwrap(), b"\"-Xmx4G\"\n".to_vec());
}

#[cfg(not(windows))]
#[test]
fn other_platforms_write_utf8() {
    assert_eq!(encode_argfile("\"C:/Jugadores/Ñandú\"\n").unwrap(), "\"C:/Jugadores/Ñandú\"\n".as_bytes().to_vec());
}

#[cfg(windows)]
#[test]
fn windows_skips_the_argfile_when_the_code_page_cant_hold_a_path() {
    // No Windows ANSI code page holds both CJK and Cyrillic, and none maps emoji.
    assert_eq!(encode_argfile("\"C:/Users/玩家/Игрок/🎮\"\n"), None);
}

#[cfg(windows)]
#[tokio::test]
async fn an_unencodable_argfile_falls_back_to_the_command_line() {
    let tmp = tempfile::tempdir().unwrap();
    let cmd = LaunchCommand {
        args: vec![
            "-Djava.library.path=C:/玩家/Игрок/🎮".into(),
            "net.minecraft.client.main.Main".into(),
            "--accessToken".into(),
            "tok-0123456789".into(),
        ],
        ..long_command()
    };
    let file = tmp.path().join("jvm-args.txt");
    let out = with_argfile(&cmd, "net.minecraft.client.main.Main", &file, "tok-0123456789").await.unwrap();
    assert_eq!(out.args, cmd.args);
    assert!(!file.exists());
}
```

(`long_command()` is the helper the existing argfile tests use.)

Run `cargo test -p launcher-core argfile encode`. Expected: a compile error.

- [ ] **Step 3: Implement.** Add above `with_argfile`:

```rust
/// The bytes Java reads an argfile as. Java's launcher reads `@argfiles` in the system ANSI code
/// page on Windows, so they are encoded in it (or as UTF-8 when the system code page is UTF-8);
/// elsewhere it is UTF-8. `None` when some character has no form in that code page.
pub fn encode_argfile(text: &str) -> Option<Vec<u8>> {
    #[cfg(windows)]
    {
        ansi_bytes(text)
    }
    #[cfg(not(windows))]
    {
        Some(text.as_bytes().to_vec())
    }
}

#[cfg(windows)]
fn ansi_bytes(text: &str) -> Option<Vec<u8>> {
    use windows_sys::Win32::Globalization::{GetACP, WideCharToMultiByte, CP_ACP, WC_NO_BEST_FIT_CHARS};
    // SAFETY: GetACP takes no arguments and only reads system state.
    if unsafe { GetACP() } == 65001 {
        return Some(text.as_bytes().to_vec());
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return Some(Vec::new());
    }
    let mut lossy = 0;
    // SAFETY: `wide` is a valid UTF-16 buffer of the given length; a null output buffer with
    // size 0 asks only for the needed size; `lossy` is a valid out pointer.
    let needed = unsafe {
        WideCharToMultiByte(CP_ACP, WC_NO_BEST_FIT_CHARS, wide.as_ptr(), wide.len() as i32, std::ptr::null_mut(), 0, std::ptr::null(), &mut lossy)
    };
    if needed <= 0 || lossy != 0 {
        return None;
    }
    let mut out = vec![0u8; needed as usize];
    // SAFETY: `out` has exactly `needed` bytes, as measured above.
    let written = unsafe {
        WideCharToMultiByte(CP_ACP, WC_NO_BEST_FIT_CHARS, wide.as_ptr(), wide.len() as i32, out.as_mut_ptr(), needed, std::ptr::null(), &mut lossy)
    };
    (written == needed && lossy == 0).then_some(out)
}
```

In `with_argfile`, replace `write_atomic(path, argfile_text(jvm).as_bytes()).await?;` with:

```rust
    // A path the system code page can't hold: keep the plain command line (below Windows'
    // 32k limit it still starts; above it, the spawn fails with the usual start error).
    let Some(bytes) = encode_argfile(&argfile_text(jvm)) else {
        return Ok(cmd.clone());
    };
    write_atomic(path, &bytes).await?;
```

Update `with_argfile`'s doc comment to mention the fallback. If the crate has `#![deny(unsafe_code)]`, put `#[allow(unsafe_code)]` on `ansi_bytes`. If it has `forbid`, ask before changing it.

- [ ] **Step 4: Verify.** Run `cargo test -p launcher-core launch:: && cargo clippy -p launcher-core --all-targets -- -D warnings`. Expected: PASS on Windows; CI covers Linux. The existing `java_reads_the_argfile_back` test still passes.

- [ ] **Step 5: Commit** with message `fix(core): write the Windows argfile in the ANSI code page Java reads` and the trailer.

---

### Task 15: mrpack corrupt-entry code

**Files:**
- Modify: `crates/launcher-core/src/error.rs`, `crates/launcher-core/src/sources/mrpack.rs:157-300`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Regenerate: `ui/src/ipc/contract/error_codes.json`

**Interfaces:**
- Produces: `ErrorCode::MrpackCorrupt` (`"mrpack_corrupt"`, category Pack, param `file`).

- [ ] **Step 1: Write the failing test** in `mrpack.rs` tests (tiny archive):

```rust
#[test]
fn a_damaged_entry_is_mrpack_corrupt_not_a_disk_error() {
    use std::io::Write;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("pack.mrpack");
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(INDEX, deflated).unwrap();
        zip.write_all(br#"{"formatVersion":1,"game":"minecraft","versionId":"1","name":"P","files":[],"dependencies":{"minecraft":"1.20.1"}}"#).unwrap();
        zip.start_file("overrides/config/a.txt", deflated).unwrap();
        zip.write_all(&b"hello world ".repeat(64)).unwrap();
        zip.finish().unwrap();
    }
    // Flip bytes inside the second entry's compressed data (after its local header).
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes.windows(22).position(|w| w == b"overrides/config/a.txt").unwrap() + 22 + 4;
    for b in &mut bytes[at..at + 8] {
        *b ^= 0xFF;
    }
    std::fs::write(&path, &bytes).unwrap();
    let err = read_archive(&path, "stamp".into(), Hashes::sha1("0".repeat(40)), Limits::default()).unwrap_err();
    assert_eq!(err.code, ErrorCode::MrpackCorrupt, "{err:?}");
}
```

(`read_archive(path, stamp, archive, limits)` is at `mrpack.rs:178`; `Hashes` is `crate::download::Hashes`.) If flipping bytes makes the CRC check fail rather than inflate, the read still errors as an `io::Error`, which is exactly the path under test.

Run `cargo test -p launcher-core mrpack::`. Expected: FAIL (the code is `DiskIo`).

- [ ] **Step 2: Implement.**
  - Add `MrpackCorrupt,` to `error_codes!` under `// packs and sync`.
  - In `mrpack.rs`:

```rust
/// The archive itself is damaged (a broken zip, or an entry that won't inflate), which is not a
/// disk problem.
fn corrupt(file: &str, detail: impl std::fmt::Display) -> LauncherError {
    LauncherError::pack(ErrorCode::MrpackCorrupt, format!("This .mrpack file is damaged ({file})"))
        .with_param("file", file)
        .with_fix("Download the pack again")
        .with_detail(detail)
}
```

  - Change `zip_error` to `corrupt("the .mrpack", e)`.
  - In the overrides loop, map `hash_reader(...)` errors with `.map_err(|e| corrupt(&path, e))?` instead of `?`.
  - In `extract_entry`, map `source.read(&mut buf)` errors to `corrupt(entry, e)`, and change the hash mismatch to `corrupt(entry, "it changed since the pack was read")`.
  - Leave genuine file-system errors (`File::open`, `create_dir_all`, `File::create`, `write_all`) as disk errors.
  - Strings:
    - en: `"mrpack_corrupt": { "message": "This .mrpack file is damaged ({{file}}).", "fix": "Download the pack again." }`
    - es: `"mrpack_corrupt": { "message": "Este archivo .mrpack está dañado ({{file}}).", "fix": "Vuelve a descargar el pack." }`
  - Run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.

- [ ] **Step 3: Verify.** Run `cargo test --workspace && pnpm --filter ui test`. Expected: PASS. Update any existing mrpack test that expected `PackInvalid` for a broken zip.

- [ ] **Step 4: Commit** with message `fix(core): a damaged .mrpack entry reports mrpack_corrupt, not a disk error` and the trailer.

---

### Task 16: Release scripts

**Files:**
- Create: `scripts/release/package.json`, `scripts/release/lib.mjs`, `scripts/release/cli.mjs`, `scripts/release/lib.test.mjs`
- Modify: `pnpm-workspace.yaml`, `pnpm-lock.yaml` (via `pnpm install`)

**Interfaces:**
- Produces, in `lib.mjs`:
  - `nextVersion({ base, channel, tags }) -> string`;
  - `setWorkspaceVersion(cargoToml: string, version: string) -> string`;
  - `releaseName(platform, version) -> string`;
  - `channelName(platform, channel) -> string`;
  - `findBundle(paths: string[], platform) -> string | undefined`;
  - `buildManifest({ version, notes, pubDate, signatures, baseUrl }) -> object`;
  - `PLATFORMS`.
- Produces `cli.mjs` subcommands `version <channel>`, `set-version <version>`, `stage <version> <inDir> <outDir>`, `manifest <version> <dir> <baseUrl> <out>` and `channel-copies <version> <channel> <dir> <outDir>`.

- [ ] **Step 1: The package.** `scripts/release/package.json`:

```json
{
  "name": "release-scripts",
  "private": true,
  "license": "GPL-3.0-only",
  "type": "module",
  "scripts": {
    "test": "node --test"
  }
}
```

Add `  - scripts/release` to `pnpm-workspace.yaml`, then run `pnpm install` (this updates the lockfile's importers).

- [ ] **Step 2: Write the failing tests** in `lib.test.mjs`:

```js
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildManifest, channelName, findBundle, nextVersion, PLATFORMS, releaseName, setWorkspaceVersion } from './lib.mjs';

test('the first beta of a base version is beta.1', () => {
  assert.equal(nextVersion({ base: '0.2.0', channel: 'beta', tags: ['v0.1.0-beta.3'] }), '0.2.0-beta.1');
});

test('betas count up from the highest, ignoring odd tags', () => {
  const tags = ['v0.2.0-beta.1', 'v0.2.0-beta.10', 'v0.2.0-beta.2', 'v0.2.0-beta.x', 'channel-beta'];
  assert.equal(nextVersion({ base: '0.2.0', channel: 'beta', tags }), '0.2.0-beta.11');
});

test('stable is the base, once', () => {
  assert.equal(nextVersion({ base: '0.2.0', channel: 'stable', tags: ['v0.2.0-beta.4'] }), '0.2.0');
  assert.throws(() => nextVersion({ base: '0.2.0', channel: 'stable', tags: ['v0.2.0'] }), /already released/);
});

test('no beta after its stable, no bad input', () => {
  assert.throws(() => nextVersion({ base: '0.2.0', channel: 'beta', tags: ['v0.2.0'] }), /bump the version/);
  assert.throws(() => nextVersion({ base: '0.2', channel: 'beta', tags: [] }), /x\.y\.z/);
  assert.throws(() => nextVersion({ base: '0.2.0', channel: 'nightly', tags: [] }), /channel/);
});

test('the workspace version is replaced and nothing else', () => {
  const toml = '[workspace.package]\nversion = "0.2.0"\nedition = "2021"\n\n[workspace.dependencies]\nfoo = { version = "1" }\n';
  assert.equal(setWorkspaceVersion(toml, '0.2.0-beta.3'), toml.replace('version = "0.2.0"', 'version = "0.2.0-beta.3"'));
  assert.throws(() => setWorkspaceVersion('[package]\nversion = "1.0.0"\n', '1.0.1'), /workspace\.package/);
});

test('installers get safe release and channel names', () => {
  assert.equal(releaseName('windows-x86_64-nsis', '0.2.0-beta.1'), 'PerlandLauncher_0.2.0-beta.1_x64-setup.exe');
  assert.equal(releaseName('linux-x86_64-appimage', '0.2.0-beta.1'), 'PerlandLauncher_0.2.0-beta.1_amd64.AppImage');
  assert.equal(releaseName('linux-x86_64-deb', '0.2.0-beta.1'), 'PerlandLauncher_0.2.0-beta.1_amd64.deb');
  assert.equal(channelName('windows-x86_64-nsis', 'beta'), 'PerlandLauncher-beta-setup.exe');
  assert.equal(channelName('linux-x86_64-appimage', 'stable'), 'PerlandLauncher.AppImage');
  assert.equal(channelName('linux-x86_64-deb', 'beta'), 'PerlandLauncher-beta.deb');
});

test('bundles are found by platform, never their signatures', () => {
  const paths = [
    'nsis/Perland Launcher_0.2.0-beta.1_x64-setup.exe',
    'nsis/Perland Launcher_0.2.0-beta.1_x64-setup.exe.sig',
    'appimage/Perland Launcher_0.2.0-beta.1_amd64.AppImage',
    'deb/Perland Launcher_0.2.0-beta.1_amd64.deb',
  ];
  assert.equal(findBundle(paths, 'windows-x86_64-nsis'), paths[0]);
  assert.equal(findBundle(paths, 'linux-x86_64-appimage'), paths[2]);
  assert.equal(findBundle(paths, 'linux-x86_64-deb'), paths[3]);
  assert.equal(findBundle([], 'linux-x86_64-deb'), undefined);
});

test('the manifest lists every platform at the versioned release', () => {
  const signatures = Object.fromEntries(PLATFORMS.map((p) => [p, `sig-${p}`]));
  const manifest = buildManifest({
    version: '0.2.0-beta.1',
    notes: 'Closed beta',
    pubDate: '2026-10-15T10:00:00Z',
    signatures,
    baseUrl: 'https://github.com/PerlandDev/perland-launcher/releases/download/v0.2.0-beta.1',
  });
  assert.equal(manifest.version, '0.2.0-beta.1');
  assert.deepEqual(manifest.platforms['linux-x86_64-deb'], {
    signature: 'sig-linux-x86_64-deb',
    url: 'https://github.com/PerlandDev/perland-launcher/releases/download/v0.2.0-beta.1/PerlandLauncher_0.2.0-beta.1_amd64.deb',
  });
  assert.equal(Object.keys(manifest.platforms).length, 3);
});

test('a missing signature fails the manifest', () => {
  assert.throws(
    () => buildManifest({ version: '0.2.0', notes: '', pubDate: '2026-10-15T10:00:00Z', signatures: { 'windows-x86_64-nsis': 's' }, baseUrl: 'https://x' }),
    /linux-x86_64-appimage/,
  );
});
```

Run `pnpm --filter release-scripts test`. Expected: FAIL (`lib.mjs` is missing).

- [ ] **Step 3: Implement `lib.mjs`**:

```js
// Pure helpers for .github/workflows/release.yml (spec §3.2–§3.4). Tested with node:test.

/** Updater manifest keys (tauri-plugin-updater looks up `<os>-<arch>-<installer>` first). */
export const PLATFORMS = ['windows-x86_64-nsis', 'linux-x86_64-appimage', 'linux-x86_64-deb'];

const SUFFIX = {
  'windows-x86_64-nsis': { release: '_x64-setup.exe', channel: '-setup.exe', bundle: /_x64-setup\.exe$/ },
  'linux-x86_64-appimage': { release: '_amd64.AppImage', channel: '.AppImage', bundle: /\.AppImage$/ },
  'linux-x86_64-deb': { release: '_amd64.deb', channel: '.deb', bundle: /\.deb$/ },
};

export function nextVersion({ base, channel, tags }) {
  if (!/^\d+\.\d+\.\d+$/.test(base)) throw new Error(`The base version ${base} must be x.y.z`);
  const released = tags.includes(`v${base}`);
  if (channel === 'stable') {
    if (released) throw new Error(`v${base} is already released; bump the version on main`);
    return base;
  }
  if (channel !== 'beta') throw new Error(`Unknown channel ${channel}`);
  if (released) throw new Error(`v${base} is already stable; bump the version on main before a new beta`);
  const prefix = `v${base}-beta.`;
  const numbers = tags
    .filter((tag) => tag.startsWith(prefix))
    .map((tag) => tag.slice(prefix.length))
    .filter((n) => /^\d+$/.test(n))
    .map(Number);
  return `${base}-beta.${Math.max(0, ...numbers) + 1}`;
}

/** Set `[workspace.package] version`, leaving every other `version =` alone. */
export function setWorkspaceVersion(cargoToml, version) {
  const section = /(\[workspace\.package\][^[]*?\nversion = ")[^"]*(")/;
  if (!section.test(cargoToml)) throw new Error('No [workspace.package] version in Cargo.toml');
  return cargoToml.replace(section, `$1${version}$2`);
}

export function releaseName(platform, version) {
  return `PerlandLauncher_${version}${SUFFIX[platform].release}`;
}

export function channelName(platform, channel) {
  return `PerlandLauncher${channel === 'beta' ? '-beta' : ''}${SUFFIX[platform].channel}`;
}

export function findBundle(paths, platform) {
  return paths.find((p) => SUFFIX[platform].bundle.test(p));
}

export function buildManifest({ version, notes, pubDate, signatures, baseUrl }) {
  const platforms = {};
  for (const platform of PLATFORMS) {
    const signature = signatures[platform];
    if (!signature) throw new Error(`No updater signature for ${platform}`);
    platforms[platform] = { signature, url: `${baseUrl}/${releaseName(platform, version)}` };
  }
  return { version, notes, pub_date: pubDate, platforms };
}
```

Run `pnpm --filter release-scripts test`. Expected: PASS.

- [ ] **Step 4: `cli.mjs`** (thin I/O, no logic worth testing):

```js
#!/usr/bin/env node
// CLI for the release workflow. Every decision is in lib.mjs.
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { buildManifest, channelName, findBundle, nextVersion, PLATFORMS, releaseName, setWorkspaceVersion } from './lib.mjs';

const root = new URL('../../', import.meta.url);
const cargoPath = new URL('Cargo.toml', root);

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]));
}

const [command, ...args] = process.argv.slice(2);
switch (command) {
  case 'version': {
    const base = /\[workspace\.package\][^[]*?\nversion = "([^"]+)"/.exec(readFileSync(cargoPath, 'utf8'))?.[1];
    const tags = execFileSync('git', ['tag', '--list', 'v*'], { encoding: 'utf8' }).split('\n').filter(Boolean);
    console.log(nextVersion({ base, channel: args[0], tags }));
    break;
  }
  case 'set-version':
    writeFileSync(cargoPath, setWorkspaceVersion(readFileSync(cargoPath, 'utf8'), args[0]));
    break;
  case 'stage': {
    // Copy each built installer and its .sig to outDir under its release name.
    const [version, inDir, outDir] = args;
    mkdirSync(outDir, { recursive: true });
    const paths = walk(inDir);
    let staged = 0;
    for (const platform of PLATFORMS) {
      const bundle = findBundle(paths, platform);
      if (!bundle) continue;
      copyFileSync(bundle, join(outDir, releaseName(platform, version)));
      copyFileSync(`${bundle}.sig`, join(outDir, `${releaseName(platform, version)}.sig`));
      console.log(`${relative(inDir, bundle)} -> ${releaseName(platform, version)}`);
      staged += 1;
    }
    if (staged === 0) throw new Error(`No installers under ${inDir}`);
    break;
  }
  case 'manifest': {
    const [version, dir, baseUrl, out] = args;
    const signatures = Object.fromEntries(
      PLATFORMS.map((p) => [p, readFileSync(join(dir, `${releaseName(p, version)}.sig`), 'utf8').trim()]),
    );
    const notes = 'Closed beta. Download it from https://perland.online/launcher';
    const manifest = buildManifest({ version, notes, pubDate: new Date().toISOString(), signatures, baseUrl });
    writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
    break;
  }
  case 'channel-copies': {
    const [version, channel, dir, outDir] = args;
    mkdirSync(outDir, { recursive: true });
    for (const platform of PLATFORMS) {
      copyFileSync(join(dir, releaseName(platform, version)), join(outDir, channelName(platform, channel)));
    }
    break;
  }
  default:
    throw new Error(`Unknown command ${command}`);
}
```

Smoke-test it locally: run `node scripts/release/cli.mjs version beta` and expect `0.2.0-beta.1` (Task 5 set the version). Run `node scripts/release/cli.mjs set-version 0.2.0-beta.1 && git diff --stat Cargo.toml && git checkout Cargo.toml`.

- [ ] **Step 5: Verify.** Run `pnpm -r test && pnpm -r lint && pnpm -r typecheck`. Expected: PASS (the package has only a `test` script, so the others skip it).

- [ ] **Step 6: Commit** with message `build: release scripts (version, staging, updater manifest) with node:test` and the trailer.

---

### Task 17: Release workflow; retire the tester build

**Files:**
- Create: `.github/workflows/release.yml`
- Delete: `.github/workflows/tester-build.yml`
- Modify: `README.md` (a "Releasing" section), `CLAUDE.md` (one line)

**Interfaces:**
- Consumes: `scripts/release/cli.mjs` (Task 16), the secrets from P1, and `tauri.conf.json`'s bundle and updater config (Task 5).

- [ ] **Step 1: Write `.github/workflows/release.yml`**:

```yaml
name: Release
# Publishes a launcher release (spec §3.4). Run it from the Actions tab → Release → Run workflow:
# pick the channel and untick "Dry run" to publish. Beta may run from any branch; stable only
# from main. Each publish is public.
on:
  workflow_dispatch:
    inputs:
      channel:
        description: Update channel
        type: choice
        options: [beta, stable]
        default: beta
      dry_run:
        description: Dry run (build only, publish nothing)
        type: boolean
        default: true

permissions:
  contents: read

concurrency:
  group: release
  cancel-in-progress: false

jobs:
  plan:
    runs-on: ubuntu-latest
    timeout-minutes: 5
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - name: Stable comes from main only
        if: inputs.channel == 'stable' && github.ref != 'refs/heads/main'
        run: |
          echo "::error::Stable releases are published from main only."
          exit 1
      - name: The updater signing key is set
        env:
          KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
        run: |
          if [ -z "$KEY" ]; then
            echo "::error::Add the TAURI_SIGNING_PRIVATE_KEY secret first."
            exit 1
          fi
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          fetch-tags: true
      - uses: actions/setup-node@v4
        with:
          node-version: 24
      - id: version
        env:
          CHANNEL: ${{ inputs.channel }}
        run: echo "version=$(node scripts/release/cli.mjs version "$CHANNEL")" >> "$GITHUB_OUTPUT"

  build:
    needs: plan
    strategy:
      fail-fast: true
      matrix:
        include:
          - os: windows-latest
            bundles: nsis
          - os: ubuntu-22.04
            bundles: appimage,deb
    runs-on: ${{ matrix.os }}
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4
      - name: Linux webview build dependencies
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 24
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Set the release version
        shell: bash
        env:
          VERSION: ${{ needs.plan.outputs.version }}
        run: node scripts/release/cli.mjs set-version "$VERSION"
      # Authenticode signing stays off until Azure Trusted Signing exists (spec §3.5): set the
      # repository variable AZURE_TRUSTED_SIGNING=true and the AZURE_* secrets to turn it on.
      - name: Install the signing tool
        if: runner.os == 'Windows' && vars.AZURE_TRUSTED_SIGNING == 'true'
        run: cargo install trusted-signing-cli --locked
      - name: Build and sign the updater artifacts
        shell: bash
        env:
          PERLAND_BUILD: ${{ inputs.channel == 'beta' && 'beta' || '' }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
          BUNDLES: ${{ matrix.bundles }}
          SIGN_WINDOWS: ${{ runner.os == 'Windows' && vars.AZURE_TRUSTED_SIGNING == 'true' }}
          AZURE_CLIENT_ID: ${{ secrets.AZURE_CLIENT_ID }}
          AZURE_CLIENT_SECRET: ${{ secrets.AZURE_CLIENT_SECRET }}
          AZURE_TENANT_ID: ${{ secrets.AZURE_TENANT_ID }}
          AZURE_SIGNING_ENDPOINT: ${{ vars.AZURE_SIGNING_ENDPOINT }}
          AZURE_SIGNING_ACCOUNT: ${{ vars.AZURE_SIGNING_ACCOUNT }}
          AZURE_SIGNING_PROFILE: ${{ vars.AZURE_SIGNING_PROFILE }}
        run: |
          args=(--bundles "$BUNDLES" --config '{"bundle":{"createUpdaterArtifacts":true}}')
          if [ "$SIGN_WINDOWS" = "true" ]; then
            sign="trusted-signing-cli -e $AZURE_SIGNING_ENDPOINT -a $AZURE_SIGNING_ACCOUNT -c $AZURE_SIGNING_PROFILE -d PerlandLauncher %1"
            args+=(--config "{\"bundle\":{\"windows\":{\"signCommand\":\"$sign\"}}}")
          fi
          pnpm tauri build "${args[@]}"
      - name: Stage the installers
        shell: bash
        env:
          VERSION: ${{ needs.plan.outputs.version }}
        run: node scripts/release/cli.mjs stage "$VERSION" target/release/bundle dist
      - uses: actions/upload-artifact@v4
        with:
          name: bundles-${{ runner.os }}
          path: dist/*
          if-no-files-found: error
          retention-days: 14

  publish:
    needs: [plan, build]
    if: ${{ !inputs.dry_run }}
    runs-on: ubuntu-latest
    timeout-minutes: 15
    permissions:
      contents: write
    env:
      GH_TOKEN: ${{ github.token }}
      VERSION: ${{ needs.plan.outputs.version }}
      CHANNEL: ${{ inputs.channel }}
      REPO: ${{ github.repository }}
      SHA: ${{ github.sha }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 24
      - uses: actions/download-artifact@v4
        with:
          path: dist
          pattern: bundles-*
          merge-multiple: true
      - name: Updater manifest
        run: node scripts/release/cli.mjs manifest "$VERSION" dist "https://github.com/$REPO/releases/download/v$VERSION" latest.json
      - name: Versioned release
        run: |
          notes="Closed beta. Download it from https://perland.online/launcher. The installers aren't code-signed yet, so Windows may warn before it runs."
          flags=(--latest=false)
          if [ "$CHANNEL" = "beta" ]; then flags+=(--prerelease); else flags=(--latest); fi
          gh release create "v$VERSION" --repo "$REPO" --target "$SHA" --title "Perland Launcher $VERSION" --notes "$notes" "${flags[@]}" dist/*
      - name: Channel releases
        run: |
          feed() {
            tag="$1"; title="$2"; shift 2
            if ! gh release view "$tag" --repo "$REPO" >/dev/null 2>&1; then
              gh release create "$tag" --repo "$REPO" --target "$SHA" --title "$title" --latest=false "$@" \
                --notes "The launcher's update feed. Download the launcher from https://perland.online/launcher."
            fi
          }
          node scripts/release/cli.mjs channel-copies "$VERSION" "$CHANNEL" dist channel
          cp latest.json channel/latest.json
          if [ "$CHANNEL" = "beta" ]; then
            feed channel-beta "Beta channel (updates)" --prerelease
            gh release upload channel-beta channel/* --repo "$REPO" --clobber
          else
            feed channel-stable "Stable channel (updates)"
            gh release upload channel-stable channel/* --repo "$REPO" --clobber
            # Beta players move up to a newer stable (spec §3.3); the beta installers stay.
            feed channel-beta "Beta channel (updates)" --prerelease
            gh release upload channel-beta latest.json --repo "$REPO" --clobber
          fi
```

- [ ] **Step 2: Delete the tester build.** Run `git rm .github/workflows/tester-build.yml`.

- [ ] **Step 3: Docs.** Add a `## Releasing` section to `README.md` (after the existing build or run section):
  - Actions → **Release** → Run workflow. Pick `beta` or `stable`, and untick **Dry run** to publish.
  - The version is computed: `<base>-beta.N` for beta, `<base>` for stable. The base is `[workspace.package] version` in `Cargo.toml`, and after a stable release it's bumped in a PR.
  - The required secrets are `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Authenticode signing turns on with the variable `AZURE_TRUSTED_SIGNING=true` plus the `AZURE_*` secrets and variables named in the workflow.
  - Each release is a public versioned GitHub Release. `channel-beta` and `channel-stable` hold `latest.json` and fixed-name installers; perland.online/launcher links those.
  - A dry run's installers are downloadable from the run's artifacts. That replaces the old tester build.

  Add one line to `CLAUDE.md` after "Run the app": `- Releases: the manual **Release** workflow (README → Releasing). Every publish is public, so ask the owner first.`

- [ ] **Step 4: Validate the YAML.** Run `pnpm dlx @action-validator/cli .github/workflows/release.yml` if it's available; otherwise rely on GitHub's parse on push. Check by eye that every job has `timeout-minutes` and only `publish` has `contents: write`.

- [ ] **Step 5: Commit** with message `build: one-click Release workflow (beta/stable, signed updater artifacts); retire the tester build` and the trailer.

---

### Task 18: Rebase, gate and draft PR

- [ ] **Step 1: Rebase.** Run `git fetch origin && git rebase origin/main`. Resolve any conflicts with the "Fix beta feedback" session's changes in `tauri.conf.json` and `main.rs` by keeping both sides' intent, then regenerate the contract (`UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`).
- [ ] **Step 2: Run the full gate**, in order: `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`. Expected: all pass. Fix anything and commit the fix.
- [ ] **Step 3: Push and open a draft PR** against `main`:
  - **Title:** "Plan 6: installers, signed auto-update with a beta channel, launcher log and diagnostics".
  - **Body:**
    - what changed, per layer;
    - the owner decisions (spec §2);
    - the Review Focus list;
    - the end-to-end checklist (Task 20) as unchecked boxes;
    - Task 12's screenshots;
    - the follow-ups (§6.2, plus anything deferred).
  - End the body with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
  - Then follow the ccd_pr instructions: `get_status`, `bind_pr` if needed, and offer Auto-fix.

---

### Task 19: Dry run (controller)

- [ ] **Step 1: Confirm P1.** Ask the owner to confirm both secrets are set. Never ask for their values.
- [ ] **Step 2: Make the workflow dispatchable.** GitHub only offers `workflow_dispatch` for a workflow file that exists on the default branch, but a run started with `--ref <branch>` uses that branch's copy of the file and scripts. So, with the owner's OK, open a one-file PR against `main` that adds this same `release.yml`, and have the owner merge it. It is inert on `main`: it is never dispatched there before the full PR lands, and a stable run from `main` would fail at the script step until then. This keeps spec decision 8 (test releases from the PR branch, before the merge). If the owner prefers not to, the end-to-end test runs from `main` after the PR merges instead; record which happened.
- [ ] **Step 3: Start the dry run** on the PR branch: `gh workflow run release.yml --ref claude/plan-6-distribution -f channel=beta -f dry_run=true`.
- [ ] **Step 4: Check the artifacts.** When the run finishes, download them and check:
  - `PerlandLauncher_0.2.0-beta.1_x64-setup.exe` (and `.AppImage`, `.deb`) each have a non-empty `.sig`;
  - the Windows job's log shows NSIS with `currentUser`;
  - nothing in the logs echoes a secret.
- [ ] **Step 5: Fix and re-run** until it's clean. Commit any fixes to the branch.

---

### Task 20: End-to-end beta (owner gates; public)

Each publish step needs the owner's explicit go-ahead in chat, quoted in the PR's checklist. Record each result in the PR body.

- [ ] **Step 1: Publish beta.1.** Ask: "Publish 0.2.0-beta.1 publicly from the PR branch?" On yes, run `gh workflow run release.yml --ref <branch> -f channel=beta -f dry_run=false`, then check that the `v0.2.0-beta.1` pre-release and `channel-beta` (with `latest.json` and the three fixed-name installers) exist.
- [ ] **Step 2: Install on Windows** from `https://github.com/PerlandDev/perland-launcher/releases/download/channel-beta/PerlandLauncher-beta-setup.exe`:
  - there's no admin prompt;
  - SmartScreen shows "More info → Run anyway";
  - it installs under `%LOCALAPPDATA%`;
  - the app opens, About shows "0.2.0-beta.1 · Beta build", an offline account can be added, and existing worlds are there.

  If Smart App Control blocks it, record that and continue on a machine or VM without it enforcing (spec §8).
- [ ] **Step 3: Check the deep link.** On an invite page, press the Perland Launcher button: `perland://join` opens the installed app on the Join server screen.
- [ ] **Step 4: Check the update feed.**
  - Ask: "Publish 0.2.0-beta.2?" On yes, run the workflow again (same command).
  - Start the installed beta.1. Within a few seconds the title bar shows "Update ready · installs when you close".
  - Start a game: Restart now is disabled. Close the game, quit the launcher, and reopen it: About shows 0.2.0-beta.2, with worlds and accounts intact.
  - Confirm the reopen happened only because you reopened it (no relaunch on quit).
- [ ] **Step 5: Check Restart now.** Ask: "Publish 0.2.0-beta.3 to test Restart now?" On yes, publish it, start the installed beta.2, and wait for the title-bar note. Click **Restart now**: the installer runs, and the launcher relaunches by itself on 0.2.0-beta.3. If the owner declines, leave this box unchecked and note it in the PR as untested.
- [ ] **Step 6: Check the empty stable channel.** In Settings, switch to Stable: the status says "Up to date" (Review Focus 1), never "Couldn't check". Switch back to Beta.
- [ ] **Step 7: Linux spot check** (a VM or the owner's Linux box, if there is one): the AppImage starts and `perland://` registers; the `.deb` installs.
- [ ] **Step 8: Check diagnostics.** Export diagnostics from the installed build, open the zip, and confirm there's no home path, no invite token and no `accounts.json`.
  - [ ] The installed build's `logs/launcher/launcher.<date>.log` has the startup line and no home path.

---

### Task 21: PerlandDev-site download page

**Repository:** `E:\PerlandDev-site` (its own branch and PR, after Task 20 step 1 so its links resolve). It is styled through that repo's `/impeccable` surface `.impeccable/surfaces/src-pages-launcher-astro.md`. The strings are in `src/i18n/launcher.ts`, and the markup in `src/components/launcher/LauncherPage.astro`.

- [ ] **Step 1: Branch** from that repo's `origin/main` in a fresh worktree or branch (`claude/launcher-beta-download`).
- [ ] **Step 2: Run `/impeccable` there** with this brief. Use the draft copy below; impeccable may tighten it within the voice in `launcher.ts`'s header comment.
  - **The hero button** becomes the download: link `https://github.com/PerlandDev/perland-launcher/releases/download/channel-beta/PerlandLauncher-beta-setup.exe`.
    - en: `cta` "Download the beta", `ctaSub` "Windows 10 and 11 · closed beta".
    - es: "Descarga la beta", "Windows 10 y 11 · beta cerrada".
  - **A secondary Linux line,** linking `…/channel-beta/PerlandLauncher-beta.AppImage` and `…/channel-beta/PerlandLauncher-beta.deb`.
    - en "Also on Linux: AppImage · .deb"; es "También en Linux: AppImage · .deb".
  - **The "Get notified" nav link** points at the download. The waitlist section stays lower down.
    - en: `close.title` "Get notified when it's out of beta", `close.body` "Leave your email and we'll write once, when the stable version is out. Nothing else.", `form.success` "Done. We'll write to {email} when it's out of beta."
    - es: "Te avisamos cuando salga de la beta", "Deja tu correo y te escribimos una vez, cuando salga la versión estable. Nada más.", "Listo. Le escribiremos a {email} cuando salga de la beta."
  - **A "Windows warned you?" disclosure** near the button.
    - en:
      - "Windows protected your PC: choose **More info**, then **Run anyway**. The installer isn't code-signed yet."
      - "Smart App Control can block unsigned apps with no way past. Signing is on its way; until then, try another PC."
      - "Coming from a tester .exe? Delete the old file. Your worlds and accounts stay."
    - es:
      - "Windows protegió tu PC: elige **Más información** y luego **Ejecutar de todas formas**. El instalador todavía no está firmado."
      - "El Control inteligente de aplicaciones puede bloquear apps sin firma y no deja continuar. La firma ya viene en camino; mientras tanto, prueba en otra PC."
      - "¿Vienes de un .exe de prueba? Borra el archivo viejo. Tus mundos y cuentas se quedan."
  - **Status, "Works today":** add "Forge and NeoForge", "Modpacks that stay in sync on their own", "Browse Modrinth for mods, packs and shaders", "Join a Perland server straight from its invite link", and "Installer and automatic updates, on Windows and Linux". **"Coming next":** "Sign in with Microsoft", "Signed installers, so Windows stops warning", "macOS". Write the es equivalents in the same voice, and update the status `lead` to say it's in closed beta.
  - **The servers section:** the Play with Perland Launcher button now exists on invite pages, so reword it to the present tense.
  - **`meta.description` and `hero.soon`:** no longer say "Coming to Windows first".
- [ ] **Step 3: Verify.** Run `pnpm build` in that repo, preview `/launcher` and `/es/launcher` at 375 px and desktop width, click the download link (it resolves to the installer), and screenshot the result.
- [ ] **Step 4: Commit** with the Opus trailer and open the PR in `PerlandDev/PerlandDev-site`, with a body ending in `🤖 Generated with [Claude Code](https://claude.com/claude-code)`. Note that `LAUNCHER_DOWNLOAD_URL` stays `https://perland.online/launcher`.

---

### Task 22: Finish

- [ ] **Step 1: Mark the PR ready.** Once Task 20's boxes are checked, move the launcher PR out of draft. The owner merges it, and the site PR merges after it.
- [ ] **Step 2: Update the memory note** `plan-6-distribution.md` (and its `MEMORY.md` line) with:
  - the PRs;
  - the decisions not obvious from the code (spec §2);
  - how the end-to-end run went (Smart App Control outcome, branch-dispatch outcome);
  - the follow-ups.
- [ ] **Step 3: Propose the hardening session** (spec §6.2) with `start_session` `own_initiative`: object-cache GC, murmur2-only cache keys, packwiz `meta_*` → pack codes, and English fragments in error params, plus any minors deferred during this plan.
- [ ] **Step 4: Write the durable findings to Dosu,** for example "the Tauri updater's relaunch is fixed at build time" and "the beta channel's manifest URL", if they aren't already recorded.
