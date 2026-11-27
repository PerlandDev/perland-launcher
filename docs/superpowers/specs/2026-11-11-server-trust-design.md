# Server trust: the step before a server's modpack installs

Date: 2026-11-11. Status: built (this change). Owner request: "we should be doing more warnings for the users when they join a server and the modpack so that they are aware they should only join servers and auto updates from servers they trust".

## What the player sees

1. **The trust step**, before anything from a server's modpack is downloaded or installed: the server's name, who runs it (the team's display name), how many files install on this computer, how many come from Modrinth, CurseForge, other websites, and how many the server's owner uploaded, and one sentence: mods are programs that run on your computer with your own access, so only join servers you trust. One lit key (Trust and add / link / play), one way out (Cancel / Not now) that installs nothing. One choice: "Update this server's pack automatically".
2. **Pack updates** show where each new or changed file comes from. Code that does not come from Modrinth or CurseForge is listed by name, always in view, and such an update **never installs without the player seeing it**, even with automatic updates on.
3. **Instance settings → Server → Trust**: whether the player trusts the server, "Stop trusting this server", and the automatic-update switch.

Words: calm and factual. Never "safe", "clean", "verified" or "virus-free" for a pack or a mod: nobody has verified that.

## Where it applies

| Way in | How |
|---|---|
| Invite link, `perland://join` | Join server reads `join_trust` once the card shows; the step sits under the card and the answer goes with `join_add` / `server_link` / `server_relink`. |
| Public server list, Your servers | Play asks `join_trust` before `join_add`; the step is a dialog; Not now adds nothing. |
| Any Play (Home, Continue, the ⋯ menu, a list card's existing instance) | The core refuses the run with `server_trust_needed` (`params.id`, and `join` when Play was to join). `pushError` turns that into the dialog, which records the answer (`instance_trust_set`) and plays again. |

No step for: a server without a modpack (vanilla); a server the instance already trusts; a server the signed-in player owns (their free server, or an Owner entry in Your servers), which is trusted quietly at its first Play.

## The record

`Instance.server_trust: { server, at }`, keyed by `instances::server_key(view, engine)`: the server's join name, else its address, else the Engine host and the server's own name. Not the invite token, which an owner can regenerate: a new link to the same server keeps the trust, a link to another server drops it. Unlinking drops it. `instance_update` can't change it (only the trust commands do). The automatic-update choice is the existing `update_policy` (`auto` / `ask`; a server is never `pinned`).

## The update rule

- `Gate` (`launcher-app/src/trust.rs`): untrusted + modpack + not owner → refused. Untrusted but the server can't be read and a version is installed → the run plays that version and checks no update (`accept: None` re-syncs only what's installed), so nothing new installs.
- `decide_update` (`runs.rs`): a server update is classified with the server's mod list. With `auto` (or an update-only run) it installs unseen only when no added or changed file is code from outside Modrinth and CurseForge; otherwise it waits on "Update ready" with `held: true`.
- Classification (`sync/origin.rs`): an index file's download host (modrinth.com → Modrinth, forgecdn.net / curseforge.com → CurseForge, anything else → that website). A file inside the `.mrpack` (an override) is "uploaded by the owner" when the mod list names it as uploaded; CurseForge when the list has CurseForge mods and no web or unknown ones; otherwise "inside the server's pack file", which counts as not from a catalogue. Code = any `.jar`, or anything under `mods/`.

## Decisions

See the pull request's "Decisions I made" for each one and what it costs if wrong.

## Seam for the mod-safety scan (not built)

PerlandEngine's scanner (PerlandEngine#649 and the design `docs/superpowers/specs/2026-11-11-mod-safety-scan-design.md` there) will add `pack.safety` to the invite view, a verdict per `/mods` entry and `POST /api/v1/mod-safety/lookup`. None is called or invented here. Where they go:

- `launcher-core/src/invite/mods.rs` `InviteMod`: the per-entry verdict, read leniently beside `source`.
- `launcher-app/src/dto.rs` `TrustPrompt`: the pack's safety summary; `UpdateFile`: a per-file verdict; `PackUpdate`: a "Perland removed N files" count.
- `ui/src/features/trust/TrustFacts.tsx` `ROWS`: more rows in the "Where they come from" well; `ui/src/features/home/ChangeSummary.tsx`: a verdict line under each called-out file and a removed-files line.
