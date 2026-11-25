# Personalize: rooms, button colour and your own room

Date: 2026-11-25. Status: approved by the owner (prototype reviewed in chat on 2026-11-25).

## Goal

Players asked to make the launcher look more like theirs. Everything about the look moves to one place, **Personalize**,
opened from a brush in the title bar, next to the gear. It adds three rooms, a button colour that goes with any room, and
a room made from two colours of the player's own.

Not in this version: colours unlocked by medals (they need a decision on which medal gives which colour and, likely, the
Engine to keep them on the account).

## Where

- **Title bar:** a brush button (`pixelarticons` `Brush`) between the account and the gear, labelled "Personalize" /
  "Personalización". It sits pressed (`aria-current="page"`) while Personalize is open, as the account does for the
  Profile.
- **Route:** `{ name: 'personalize' }`, a screen in the `ScreenFrame` like Settings.
- **Settings → Appearance** keeps its heading and holds one line and a button, "Open Personalize", so a player who
  looks there still finds the look.

## The screen, top to bottom

1. **Preview:** Play, an ordinary button and a running lamp, on a panel, so a choice shows at once.
2. **Theme** (the UI's word for a room; "Sala" until 2026-11-25): the swatch keys of every made room: Laminate,
   Stone, Night, Forest, then the new Nether, End and Cherry, tagged New. Below them one wide key, **Customize theme**,
   puts on the player's own room (`aria-pressed`, the lime lamp while it is on).
3. **Your colours** (only while Your room is chosen): two colour pickers, Background and Buttons, each with its hex
   field, the live checks and four starting ideas (Ocean, Lava, Mint, Grape).
4. **Button colour** (hidden while Your room is chosen, which has its own): "The room's", Amber, Orange, Pink, Violet,
   Cyan and Bone. Its hint says why green, red and blue are not offered: they mean running, failing and in progress.
5. **Text font**, **Instance icons** and **Screen transitions**, as they were in Settings.

Everything applies at once and saves like any setting; a failed save puts the stored look back.

## New rooms

| Room (en / es) | ground | panel | raised | line | ink | ink-dim | dirt | dirt-deep | act | act hover |
|---|---|---|---|---|---|---|---|---|---|---|
| Nether / Nether | `#1a0f0e` | `#241514` | `#301c1a` | `#4d2c27` | `#f8ece8` | `#cfb0a8` | `#6e2b22` | `#431a15` | `#ff8c2e` | `#ffa95e` |
| End / End | `#121019` | `#1a1725` | `#242033` | `#3b3552` | `#f3f0e6` | `#bdb6cf` | `#4f416d` | `#2d2543` | `#e6dfa2` | `#f3eec6` |
| Cherry / Cerezo | `#1b1216` | `#251920` | `#31212a` | `#4d3341` | `#f8eef2` | `#d0b4c1` | `#6f3a53` | `#432330` | `#ff8fc0` | `#ffb1d4` |

Each gets a landscape palette in `art/palette.ts`. They meet the same contrast floor as every room
(`styles/themes.test.ts`).

## Button colour

`accent` in settings: `room` (default: the room's own act hue), `amber`, `orange`, `pink`, `violet`, `cyan`, `bone`. It
replaces only `amber` and `amber-hi`, through `:root[data-accent='…']` after the room blocks. Lime, coral and blue never
change. Every accent keeps ground-coloured words readable (4.5:1) on its face in every made room; the test checks each
pair.

## Your room

`theme: 'custom'` with `customRoom: { background, buttons }`, two `#rrggbb` colours. `state/customRoom.ts` mixes the
whole room from them (`deriveRoom`): panel, raised and line lighten the background by 5, 11 and 22 %; ink is white
tinted 6 % by the background and ink-dim mixes 30 % of the background into ink; dirt is the background lightened 20 %
and moved 30 % to the buttons, dirt-deep halfway between; act hover lightens the buttons 25 %. The landscape palette is
mixed from the same two colours.

The checks (`roomChecks`), in the player's words, each pass or fail live:

- the background is dark (luminance at most 0.05);
- small text reads (ink-dim on raised at least 4.5:1);
- Play's words read (ground on buttons at least 4.5:1);
- green, red and blue still show (lime on ground 3:1, coral and blue-ink on raised 4.5:1).

A room that fails a check is saved (so the player keeps editing it) but never put on: the launcher shows Laminate until
it passes. Rust keeps only well-formed colours: a value that isn't `#rrggbb` loads, and saves, as the default.

## Settings and contract

`Settings` gains `accent` (an enum, unknown values load as `room`) and `customRoom` (default
`{ background: "#101820", buttons: "#ff6ad5" }`); `Theme` gains `nether`, `end`, `cherry` and `custom` (unknown still loads
as Laminate). The localStorage mirror holds all four look fields, so a start never flashes the old look. The IPC contract
is regenerated.

## Medal

"A mi estilo" ("Change the launcher's theme") also counts a button colour other than the room's.
