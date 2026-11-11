---
name: release-notes
description: Use when opening, finishing or updating a pull request in perland-launcher, when a change alters what players see or can do in the launcher, when asked for release notes, patch notes, a changelog or "what's new", or when the release-notes test fails.
---

# Release notes

After each update the launcher shows players a **What's new** dialog, in English or Spanish. Its lines come from `release-notes/`, one small file per note. Many players are 12–25, casual and Spanish-speaking: they know "skin", "instance" and "mods", and nothing about the code.

Every pull request settles this before it is ready. It either adds a note, or its description carries the line `Release notes: none (reason)`.

## Does this change get a note?

One question decides it: **after this update, will a player see, do or stop putting up with something?** Judge by what the player experiences, not by the kind of change.

| The change | What to do |
|---|---|
| Players can do something they couldn't | A note, `new` |
| Something they already use looks, works or runs better | A note, `improved` |
| Something broken in a release now works | A note, `fixed` |
| A dependency bump, build change or refactor that changes what players experience | A note for what they experience |
| A fix for a bug that never reached a release | None. No player saw it |
| A change to something whose note is not released yet | Edit that note so it stays true. Don't add another |
| A step of a feature players can't reach yet (behind a flag, not wired in) | None. The pull request that makes it reachable carries the feature's one note |
| Refactors, tests, CI, docs, release tooling, the dev harness | None |
| Polish a player wouldn't notice or miss | None |

To tell what is released, compare with the latest release tag:

```bash
git fetch origin --tags
latest=$(git tag --list 'v20*' --sort=-v:refname | head -1)
git diff "$latest" -- <the file with the bug>     # did the bug ship?
git ls-tree --name-only "$latest" release-notes/  # notes missing here are not released yet
```

## The file

`release-notes/<slug>.json`. The slug is lowercase words with hyphens that name the change for players (`skin-editor`, `blank-window-after-sleep`), not the branch.

```json
{
  "kind": "fixed",
  "en": "A skin's history no longer keeps the same version twice.",
  "es": "El historial de una skin ya no guarda la misma versión dos veces."
}
```

- One note is one thing a player would notice. A pull request with two unrelated ones adds two files. A feature gets one note, not one per part.
- A typo is fixed in the same file. Never rename a released note: a new slug shows up as a new note. Deleting a file removes the note from history.

## The sentence

Each of `en` and `es` is **one sentence, at most 140 characters, plain text**, that says what changed for the player:

1. Start from what the player does or sees: "You can now…", "Saving a skin…", "The memory slider…".
2. Say the one change. For a feature, name it and where to find it. Leave the list of its tools to the feature itself.
3. Use the launcher's own words for screens and buttons, as `ui/src/i18n/en.json` and `es.json` spell them (Skins, Play / Jugar, Settings / Configuración). English is British, as the UI is ("colour").
4. For `fixed`, say what works now, from their side: "…no longer…" or "…now…". The cause stays in the commit.
5. When a change helps only some players, say who in their terms: "On Windows, …", "On slow disks, …".

Nothing a player wouldn't say: names from the code, error codes, libraries, version and pull request numbers, lab figures ("40% faster"), links, emoji, markup or exclamation marks.

| Instead of | Write |
|---|---|
| Fixed XSTS error 2148916238 mapping to AccountChild. | Signing in with a child's Microsoft account now tells you what to do. |
| Parallelised asset verification (40% faster on HDDs). | The game starts sooner the first time you play a new Minecraft version. |
| Skin editor with pencil, eraser, fill, picker, mirror, undo, layers and 3D preview. | You can now draw your own skin: open Skins and choose Edit. |
| Bumped tauri to 2.12.3. | On Windows, the launcher's window no longer goes blank after your computer wakes up. |

**Spanish is written, not translated.** Say the same thing the way a player in Latin America would: informal "tú", neutral words ("computadora", "presiona", "archivo"), and the terms the UI already uses ("launcher", "skin", "instancia"). It has its own 140 characters, and Spanish runs longer, so write it shorter rather than trimming a translation.

## Before the pull request is ready

- `pnpm --filter release-scripts test` passes: it checks every note's slug, kind, both languages and length.
- Read the note as a player would see it: `pnpm --filter ui dev:mock`, then Settings → Launcher updates → What's new, under "Next update". Switch the language and read it again.
- The description says which it is: the note's text, or `Release notes: none (reason)` with a reason someone can check ("refactor, no behaviour change", "fixes a crash in the skin editor, which is not released yet").
