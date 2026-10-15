# Perland Launcher: agent guide

- Design: `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (core) and `docs/superpowers/specs/2026-10-10-launcher-desktop-ui-design.md` (desktop app). Plans: `docs/superpowers/plans/`.
- `crates/launcher-core` holds all launcher logic. `crates/launcher-app` holds the app's state and run state machine. Neither may depend on Tauri or UI crates. `src-tauri` is wiring only and has no tests. `crates/perland-cli` is a dev harness. `ui/` is the React app.
- TDD: write the failing test first. Pure logic gets `#[cfg(test)]` unit tests; HTTP behaviour gets `tests/*.rs` with wiremock; UI gets Vitest + Testing Library.
- Never run fuzzing, stress or load tests locally. They run in CI only. Keep local test inputs tiny.
- Errors shown to players are `LauncherError` (category + stable `code` + plain message + optional fix + detail + params). The UI translates by `code`. Never put tokens in logs.
- Every UI string goes in both `ui/src/i18n/en.json` and `ui/src/i18n/es.json` (neutral Latin American Spanish, informal "tú"). A new `ErrorCode` needs `errors.<code>.message` in both.
- `ui/src/ipc/contract/*.json` is generated: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit it.
- UI styling and design-system work goes through the `/impeccable` skill. The visual direction is `.impeccable/surfaces/ui-src-app-tsx.md`, and the design system is `DESIGN.md`.
- Run the app: `pnpm install`, then `pnpm --filter ui dev:mock` (browser, mocked backend) or `pnpm tauri dev` (real app; set `PERLAND_LAUNCHER_ROOT` to a scratch folder).
- Releases: the manual **Release** workflow (README → Releasing). Every publish is public, so ask the owner first.
- License GPL-3.0-only. Never copy Modrinth branding or Mojang assets. Ported code keeps an attribution header and gets a `NOTICE` entry.
- Before committing: `pnpm --filter ui build` (Tauri embeds `ui/dist`), `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.
- Issues by `perland-bot-eve` (label `discord`) were reported on Discord. When work touches one, keep the reporter posted through Perlita with the `reporter-updates` skill (`.claude/skills/reporter-updates/SKILL.md`; the same file is in PerlandEngine and PerlandPanel). This repository is public, so every comment here is too.
