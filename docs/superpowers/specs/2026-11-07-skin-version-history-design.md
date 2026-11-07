# Skin version history

Date: 2026-11-07. Builds on `2026-11-07-skin-editor-design.md`.

## What the player gets

Every wardrobe skin keeps a short history of earlier versions. In the Skins screen, **History** swaps the
texture panel for a list of versions, newest first: the face, the time, and why it was kept (edited, worn,
exported, restored). Picking one previews it on the stage (3D figure and flat texture, painting off). From the
preview the player can **Restore** it or **Make a copy** as a new wardrobe skin. **Back to now** leaves the
preview. Nothing is ever lost by restoring: the version being replaced is kept first, and the restore is one
stroke in the editor's undo, so Ctrl+Z takes it back right away.

## When a version is taken

Autosave writes 600 ms after each stroke, so a version is never taken per save. A version is the texture as it
stood at a moment, and is taken:

1. **When an editing session starts.** On a save, if the skin was last changed 5 minutes ago or more (or has
   no versions yet), the texture about to be overwritten is kept first, dated when it was last changed. So
   the state before every session, including the original import or creation, is kept, even if the launcher
   was closed mid-session.
2. **When the player leaves a skin they changed** (switches skin or leaves the screen): the UI calls
   `skin_checkpoint`.
3. **On export** (`skin_export`) and **on wear** (`skin_wear`). Worn versions are marked `worn`.
4. **On restore**: the current texture is kept, then the restored texture is kept as a `restored` version.

A version identical to the newest one (same SHA-256) is not added again; a wear of it just marks it worn.

## Cap and pruning

Each skin keeps its newest 30 unworn versions and its newest 10 worn ones; older ones are deleted after each
new version. A PNG is a few KB, so a skin's history stays well under a megabyte.

## Storage

Additive and keyed by skin id, so it doesn't depend on how `library.json` lists skins (per-account wardrobes
change that list, not ids):

```
<root>/skins/versions/<skin id>/versions.json
<root>/skins/versions/<skin id>/<version id>.png
```

A version id is the Unix time in milliseconds it was taken (digits only, made unique), and `versions.json`
holds `{ versions: [{ id, taken, reason, worn, sha256 }] }`. Every write is atomic (`fsutil::write_atomic`) and
happens under the wardrobe's lock. A damaged `versions.json` is set aside as `versions.json.damaged` and the
index is rebuilt from the PNGs in the folder (reason `edited`, not worn), so the screen always opens. Index
entries whose PNG is missing are dropped.

- **Delete** a skin: its versions folder is deleted too.
- **Duplicate** or **make a copy** from a version: the new skin has a fresh id and starts with no history.

## Commands

New commands; no existing signature changes.

| Command | Returns |
| --- | --- |
| `skin_versions(id)` | `SkinVersion[]`, newest first: `{ id, taken, reason, worn, current }` |
| `skin_version_texture(id, version)` | the version's PNG as a data URL |
| `skin_restore(id, version)` | the updated `SkinEntry` |
| `skin_version_copy(id, version, name)` | the new `SkinEntry` |
| `skin_checkpoint(id)` | nothing |

`current` marks the version whose texture is the skin's texture now. An unknown version is the new error
`skin_version_not_found`.
