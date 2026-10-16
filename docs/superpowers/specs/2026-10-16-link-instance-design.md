# Perland Launcher: Link an Existing Instance to a Server, Design Addendum

- **Date:** 2026-10-16
- **Status:** Approved by the owner (2026-10-16: "go with your recommendations")
- **Builds on:** `docs/superpowers/specs/2026-10-11-join-by-link-design.md` (server instances, `server_relink`) and the instance import of round 3 (`docs/superpowers/plans/2026-10-16-beta-feedback-3.md`, #38).

## 1. Goal

Players can now import an instance from another launcher, but it stays a plain instance: it never gets the server's updates. **Link to a server** turns a vanilla or imported instance into a server instance. From then on it updates from the server's invite like one made with Join server, and keeps its worlds, settings and screenshots.

## 2. Owner decisions (2026-10-16)

1. **Old mods are set aside, not kept.** On the first sync after linking, every `mods/*.jar` (top level) that the server's pack doesn't install at that path moves out of `mods/`. Two copies of one mod usually crash the game, and an imported instance is full of jars under other file names.
2. **The player's configs stay.** The pack's override files that already exist are kept as the player's (the planner's existing rule for files sync never wrote), and the plate says "Kept your version of N files".
3. **Only vanilla and imported instances** (`source.kind == none`) offer **Link to a server**. Server instances keep **Use a new link**; other pack instances (packwiz, Modrinth, `.mrpack`) are refused with `link_has_pack`.

## 3. Decisions this addendum makes

### 3.1 Where old mods go

- They move to `minecraft/mods-before-link/` inside the instance, not to `.perland/trash/`: the trash is purged after 7 days, and these are the player's own files. **Open folder** shows them.
- A name already taken there gets `-2`, `-3`, … before `.jar`.
- A jar at a path the pack installs is handled by the planner as before (identical: kept; different: replaced, and the old one goes to the trash).
- Nothing moves until every file is staged and the loader is installed, the same point where sync moves anything else. A failed or cancelled first sync leaves `mods/` as it was.

### 3.2 The adopt marker (core)

- `launcher_core::sync::mark_adopt(perland_dir)` writes `.perland/adopt`. `server_link` calls it.
- `sync_inner` sets mods aside only when the marker exists **and** there is no `managed.json` (a never-synced instance). It removes the marker after `managed.json` is written.
- A marker keeps the rule explicit: a pack instance that lost its `managed.json`, or one the player added jars to before its first Play, never loses mods.
- `SyncReport` gains `set_aside: Vec<String>` (the game-folder paths moved), and the app's `SyncSummary` gains `setAside`. The plate after the first Play adds "Moved N old mods to mods-before-link".

### 3.3 App command

`server_link(id, link) -> Instance`, in `servers.rs` next to `server_relink`:

- Refused while the instance has a run (`busy`), like relink.
- `source` must be `none`. A server instance gets `link_has_pack` too; the UI offers it **Use a new link** instead.
- Parses the invite, reads the view and checks it is supported (`perland::supported`), then reloads the instance, checks the run again, writes the marker, and saves `source = Perland`, `server`, `update_policy = Auto`.
- The name, art, Java settings and playtime stay. The game version changes at the first Play, through sync's existing game-change path.
- Another instance already joining the same server is allowed: the player chose this one.

### 3.4 UI

- **Instance menu:** **Link to a server** on vanilla and imported instances, opening Join server in link mode (`route.linkFor`). It is disabled while a run exists.
- **Join server, link mode:** title "Link to a server", the invite field, the server card, and the button **Link**. Below the card:
  - when the server's Minecraft version or loader differs from the instance's, a warning: "This server plays Minecraft 1.21.1 with Fabric. Linking moves this instance to that version; back up your worlds first.";
  - always, a note: "On the next Play, mods that the server doesn't use move to the mods-before-link folder. Your worlds and settings stay."
- **Instance settings:** the same **Link to a server** action, in the section the Updates control would use for packs.

### 3.5 Error code

| Code | en | es |
|---|---|---|
| `link_has_pack` | This instance already gets its mods from a modpack. Fix: Link a vanilla or imported instance, or join the server as a new instance. | Esta instancia ya recibe sus mods de un modpack. Fix: Vincula una instancia vanilla o importada, o únete al servidor como una instancia nueva. |
