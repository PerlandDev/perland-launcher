# Per-account wardrobes (2026-11-07)

Follows `2026-11-07-skin-editor-design.md`. Owner requests:

1. "When I add an account my skin from my minecraft account should always show in the skin editor."
2. "Multiple accounts like offline accounts could have their own skins, also other microsoft accounts could have
   their own skins too."

## The bug

The wardrobe was one list for every account, and the Skins screen fetched a Microsoft account's real skin only while
seeding an **empty** wardrobe. Opening the screen with no account (or an offline one) seeded the Perlander, so the
list was never empty again: a Microsoft account added later never had its skin fetched and the screen opened on the
Perlander.

## Storage

- Still one list, `<root>/skins/library.json`, and still one texture per skin at `<root>/skins/<id>.png`. Skin ids and
  texture paths don't change, so anything keyed by skin id (such as version history under `skins/versions/<id>/`)
  keeps working.
- The list gains `"version": 2`, and each entry gains `owner`: the dashed uuid of the account whose wardrobe it is in
  (Microsoft and offline alike). An entry with no `owner` is **loose**: made while no account was signed in.
- One file under one lock keeps every change atomic; a folder per account would have meant moving textures, and
  breaking id-keyed paths, for no gain.

## Migration (a list without `version`)

- An entry worn from the launcher (`wornBy` set) goes to that account.
- Every other entry becomes loose.
- Loose skins are adopted by the **active** account the next time it opens the Skins screen. That's the account the
  player was using, so their skins land where they expect. With no account signed in, the loose skins are the
  wardrobe shown.
- The same rule covers skins made with no account and kept after signing in.
- Except the starter the launcher seeds by itself: an entry marked `seeded` (never edited or renamed since) is dropped
  instead of adopted, so signing in after opening Skins with no account doesn't bring the Perlander along. A Perlander
  the player picks from New, or edits, is theirs and is adopted. Entries from a version 1 list carry no such mark, so
  an old untouched Perlander is still adopted once; the player can delete it.

## Behaviour

- Each account sees only its own wardrobe. The skin it wears comes first, then the newest.
- **A Microsoft account always shows its real skin.** Every time the screen shows a Microsoft account's wardrobe
  (opening it, adding an account, switching), the launcher fetches the current skin (`skin_sync_current`). If an entry
  in that wardrobe already has the same texture and arm model, it's marked worn; otherwise a new entry named
  "<name>'s skin" is added. Either way it's selected. A second visit finds the match, so nothing is duplicated.
  An account wearing Minecraft's default skin gets no entry, and an empty wardrobe shows the existing "wears the
  default" note.
- Offline accounts, and the loose wardrobe, start from the Perlander when empty.
- The Skins screen has an account switch that changes which wardrobe is shown (without changing the launcher's active
  account). Accounts → "Edit skin" opens that account's wardrobe.
- Removing an account leaves its skins on disk; adding it again brings them back.
- New, import and "from my account" add to the shown wardrobe; duplicate stays in the source's wardrobe.

## Offline skins in game: not built

Offline accounts keep their skins in the launcher only. Showing one in game is an open question for the owner (see the
PR): vanilla clients fetch skins from Mojang's session server by uuid, so an offline skin would need a mod or a
server-side plugin.
