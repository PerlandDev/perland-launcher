# What's new: release notes for players

Date: 2026-11-10. Status: the owner chose the approach and added the skill and the every-pull-request rule
(2026-11-10); built from `docs/superpowers/plans/2026-11-11-whats-new.md`.

## Intent

The launcher updates itself in the background and installs the update when the player closes it. The player
only ever sees a version number: every release, on GitHub and in the update feed, carries the same fixed line
("Closed beta. Download it from https://perland.online/launcher"). The owner wants players to be **told what
changed in each update**, in their language and in words they understand.

## Goals

- After an update, the launcher shows once what is new in it, in English or Spanish.
- A player can read the notes of earlier updates at any time.
- The same notes become the description of the GitHub Release.
- Notes are written with the change: a pull request that players would notice carries its own note, reviewed
  like the code.
- Releasing stays one click. The Release workflow still computes the version; nobody edits a file to release.

## Non-goals

- Showing the notes before the update installs (on the title bar's "update ready" note). Updates are automatic,
  so the moment that matters is after.
- Announcing releases on Discord or on perland.online. Either can read the GitHub Release later.
- Notes for the releases already out (up to `v2026.11.4`). History starts with the first release that has this.
- Pictures, links or formatting inside a note. A note is one plain sentence.
- A note on every pull request. Each one must settle the question, but an internal change settles it with
  "none" (see Writing a note).

## Anti-goals

- A dialog that gets between the player and Play. One click or Esc closes it, it never appears during first run,
  and it never opens over another dialog or while a game is starting.
- Developer wording reaching players (commit titles, pull request titles, "JVM", "IPC").
- A note in one language only.
- The same note shown twice, or a fresh install greeted with a list of changes it never lived through.
- A release step that commits to `main`. The ruleset on `main` needs an approval, so the workflow can't.

## Approaches considered

Where the words come from (the owner chose the first):

1. **Notes in the repository, written per pull request (chosen).** Reviewed, bilingual, and nothing to remember
   on release day. Costs one small file in each player-facing pull request.
2. **Typed into the Release workflow at release time.** Rejected: easy to forget changes, and both languages get
   written by hand under time pressure.
3. **Generated from pull request titles.** Rejected: developer shorthand, English only.

How a note gets its version (my call; the owner handed over the how):

1. **Worked out from the release tags at build time (chosen).** A note belongs to the first release since which
   every release has contained it. Nothing is moved or stamped by hand, and nothing is committed by the workflow.
2. **A "next update" section the workflow stamps and commits.** Rejected: the workflow cannot push to `main`.
3. **A follow-up pull request after each release that files the notes under the version.** Rejected: a chore
   after every release, and the release that just went out would not contain its own stamped notes.

Where the launcher reads the notes from (my call):

1. **Built into the launcher (chosen).** They are exactly the notes of the version that is running, and they
   show offline.
2. **Fetched from the update feed (`latest.json`).** Rejected: the feed describes the newest release, not the
   one installed, and the dialog would need the network right after an update.

## Design

### Writing a note

- One file per note in `release-notes/` at the repository root: `release-notes/<slug>.json`. One file each, so
  two pull requests never conflict. The slug (`[a-z0-9-]+`) is the note's identity.

  ```json
  {
    "kind": "new",
    "en": "The launcher now tells you what's new after each update.",
    "es": "El launcher ahora te cuenta qué hay de nuevo después de cada actualización."
  }
  ```

- `kind` is `new`, `improved` or `fixed`.
- `en` and `es` are each one plain sentence, at most 140 characters, with no markup and no links. They follow the
  rules of every UI string: short, plain, friendly; Spanish is neutral Latin American with informal "tú".
- A typo is fixed in place. A released note's file is never renamed (a new slug is a new note). Deleting a file
  removes the note from history, which is how old notes are pruned.
- A test in `scripts/release` (it already runs in CI through `pnpm -r test`) checks every file: the slug, the
  kind, both languages present, single line, the length.
- **Every pull request fills this in** (the owner's rule, 2026-11-10): it either adds a note, or its description
  says `Release notes: none (reason)`. `CLAUDE.md` and `AGENTS.md` carry the rule.
- **A skill teaches agents to write them**: `.claude/skills/release-notes/SKILL.md`. It covers what the test
  cannot check: whether a change needs a note at all, the kind, the player's words rather than the code's, and
  Spanish that is written rather than translated.

### From notes to a release

New pure functions in `scripts/release/lib.mjs` (tested in `lib.test.mjs`), with thin commands in `cli.mjs`:

- **Which releases count.** The CalVer tags (`vYYYY.M.MICRO` and `-beta.N`) that are ancestors of the commit being
  built, in version order, followed by the version being built. Old `v0.x` tags are ignored, as `nextVersion`
  already does.
- **A note's version.** The oldest release such that it and every later one contain the note's file. A note that
  no tag contains belongs to the version being built. This holds when a beta was published from a branch that
  merged later, and when a note was removed and added again.
- **The notes file** (`release-notes.json`): the releases newest first, grouped by base version (the notes of
  `2026.11.5-beta.1`, `-beta.2` and `2026.11.5` sit together under `2026.11.5`), each note with its slug, kind,
  both texts and its exact version; each group with its day (the tagged commit's day; today for the version being
  built). It holds the newest 10 base versions that have notes. It also lists every release it knows in order,
  so the launcher can compare versions without a SemVer parser.
- **The GitHub Release description.** Stable: every note of this base version. Beta: the notes new in this beta.
  Grouped by kind, English first, then Spanish, then today's line about where to download and unsigned
  installers. A release with no notes says so in one line in both languages ("No changes you'd notice. Fixes
  under the hood." / the Spanish). The same text goes in `latest.json`'s `notes`, which the launcher does not
  read.

In the Release workflow, the `plan` job (it already fetches the full history and tags) builds the notes file and
the description once, and the `build` and `publish` jobs use that copy. A release must fail rather than compute
notes from a shallow clone: without tags, every note would look new.

### In the launcher

- **Data.** The UI bundles the notes file. For a release it is the workflow's copy; in `pnpm --filter ui dev`,
  `dev:mock` and `pnpm tauri dev` it is computed on the spot from the checkout, where notes not yet released
  show under "Next update". That is the preview for whoever writes a note.
- **The marker.** The core remembers the last version whose notes this player was shown, in its own small file
  (`whats-new-seen.json`, beside `news-seen.json`). It is not a setting: the Settings screen saves the whole
  settings object and would overwrite it.
- **The rule** (a pure function in the UI, next to its tests, like `newsRules.ts`):
  - First run not finished: show nothing, and set the marker to the running version.
  - Marker equal to the running version: show nothing.
  - Marker older: show the notes whose version is newer than the marker, grouped by base version, newest first.
    A player who skipped two updates sees all three. A beta player moving from `beta.1` to `beta.2` sees only
    what `beta.2` added.
  - No marker, first run finished (a player updating into the first release with this feature): show the running
    base version's notes.
  - Marker newer than the running version (a downgrade): show nothing.
  - Marker that the notes file does not know (a beta from a branch that never merged): as with no marker, show
    the running base version's notes.
  - Nothing to show (an update with no notes): no dialog.
  - Dev builds never open the dialog by themselves.
- **When the marker moves** (my call). At once when there is nothing to show. Otherwise only when the dialog
  actually opens, as server news marks posts seen on open. Notes are never lost because something was in the way.
- **The dialog.** It uses the app's `Dialog`, like server news. The launcher tries it once per launch, when the
  first screen has settled. If first run is still going, another dialog is open (a join link, for example) or a
  game is starting at that moment, it does not open and the marker stays, so the next launch tries again. It
  never pops up later in the same session, so there is no watcher for other dialogs.

  ```
  ┌──────────────────────────────────────────────┐
  │ What's new                                   │
  │                                              │
  │ 2026.11.5 · 12 Nov                           │
  │ NEW                                          │
  │  · The launcher now tells you what's new     │
  │    after each update.                        │
  │ IMPROVED                                     │
  │  · Skins save faster.                        │
  │ FIXED                                        │
  │  · A skin's history no longer keeps the      │
  │    same version twice.                       │
  │                                              │
  │              [ All updates ]   [ Got it ]    │
  └──────────────────────────────────────────────┘
  ```

  The list scrolls on its own so the buttons stay in view. Notes render as text, never as HTML. The language
  follows the launcher's language. "All updates" opens the same dialog with every version in the notes file.
- **Settings → Launcher updates** gets a **What's new** button that opens the dialog with every version. In a dev
  build it is the only way in, and it shows "Next update".
- Every string goes in `en.json` and `es.json` (Spanish: "Novedades del launcher" for the title, since
  "Novedades" alone already names server news).
- The mock backend (`dev:mock`) gets the marker command, with a way to preview the dialog as if just updated.

### Docs

`README.md` → Releasing explains the notes folder and what the workflow does with it. `CLAUDE.md` and `AGENTS.md`
get the rule above and point at the `release-notes` skill.

## Left to the builder

- The exact look of the dialog and the Settings button, through the `/impeccable` skill and `DESIGN.md`.
- How the UI bundles the notes file (a Vite virtual module, or a generated file kept out of Git) and how the
  workflow hands the `plan` job's copy to the other jobs.
- The names of commands, components and i18n keys.
- The first note: this feature's own (`release-notes/whats-new.json`, the example above).

## Testing

- `scripts/release/lib.test.mjs`: a note's version (plain, beta then stable, branch beta merged later, removed and
  re-added, no tags), grouping and the 10-version cap, the release description for stable, beta and empty, and
  the validation of note files.
- Vitest: the rule (each case above), when the marker moves, a launch where something is in the way, the dialog
  (groups, both languages, plain text), the Settings button.
- Core and app: the marker's round trip, an old `settings.json` without it, and the IPC contract.
- A dry run of the Release workflow shows the description and the bundled notes before anything is published.
