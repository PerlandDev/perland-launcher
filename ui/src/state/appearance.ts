import { accent as accentSchema, theme as themeSchema, uiFont, type Accent, type CustomRoom, type Theme, type UiFont } from '../ipc/schema';

import { customRoom as customRoomSchema } from '../ipc/schema';
import { deriveRoom, isHex, roomOk } from './customRoom';

/** Everything Personalize sets (spec 2026-11-25): the room, the button colour, the player's own room's two
 *  colours, and the text font. */
export interface Appearance {
  theme: Theme;
  accent: Accent;
  customRoom: CustomRoom;
  font: UiFont;
}

/** Every room, in the order Personalize offers them. The first is the default; `custom` is the player's own. */
export const THEMES = themeSchema.options;
/** The rooms written in app.css, which the contrast test holds to the floor. `custom` is mixed at runtime instead. */
export const MADE_THEMES = THEMES.filter((theme) => theme !== 'custom');
/** Every button colour, `room` (the room's own act hue) first. */
export const ACCENTS = accentSchema.options;
export const FONTS = uiFont.options;

export const DEFAULT_APPEARANCE: Appearance = {
  theme: 'laminate',
  accent: 'room',
  customRoom: { background: '#101820', buttons: '#ff6ad5' },
  font: 'hyperlegible',
};

/** The CSS custom properties the player's own room sets on `<html>`; cleared with every other room. */
const ROOM_PROPERTIES = Object.keys(deriveRoom(DEFAULT_APPEARANCE.customRoom)).map((name) => `--color-${name}`);

/** The look part of the settings, for `applyAppearance`. */
export const lookOf = (settings: Appearance): Appearance => ({
  theme: settings.theme,
  accent: settings.accent,
  customRoom: settings.customRoom,
  font: settings.font,
});

/** A copy of the last applied look, read before the first render so a start never flashes the old theme. */
const MIRROR_KEY = 'perland.appearance';

/** Theme the whole page: app.css swaps the neutral tokens on `:root[data-theme]`, the act hue on
 *  `:root[data-accent]` and the body face on `:root[data-font]`. The player's own room has no block in the
 *  stylesheet: its tokens are mixed here and set on `<html>`, with its two colours in `data-room` so the
 *  landscapes follow. A room that fails a check is remembered but never put on (spec 2026-11-25). */
export function applyAppearance(look: Appearance) {
  const root = document.documentElement;
  const own = look.theme === 'custom' && roomOk(look.customRoom);
  root.dataset.theme = look.theme === 'custom' && !own ? 'laminate' : look.theme;
  root.dataset.font = look.font;
  // The player's own room carries its own act hue, so a button colour never overrides it.
  if (look.accent === 'room' || own) delete root.dataset.accent;
  else root.dataset.accent = look.accent;
  for (const name of ROOM_PROPERTIES) root.style.removeProperty(name);
  if (own) {
    root.dataset.room = `${look.customRoom.background},${look.customRoom.buttons}`;
    for (const [name, value] of Object.entries(deriveRoom(look.customRoom))) root.style.setProperty(`--color-${name}`, value);
  } else {
    delete root.dataset.room;
  }
  writeMirror(look);
}

/** The remembered look, or the defaults. Storage may be missing, blocked or hold anything. */
export function readMirror(): Appearance {
  let stored: unknown;
  try {
    stored = JSON.parse(localStorage.getItem(MIRROR_KEY) ?? 'null');
  } catch {
    return DEFAULT_APPEARANCE;
  }
  const record = typeof stored === 'object' && stored !== null ? (stored as Record<string, unknown>) : {};
  const theme = themeSchema.safeParse(record.theme);
  const accent = accentSchema.safeParse(record.accent);
  const font = uiFont.safeParse(record.font);
  const room = customRoomSchema.safeParse(record.customRoom);
  const picked = room.success ? { background: room.data.background.toLowerCase(), buttons: room.data.buttons.toLowerCase() } : null;
  return {
    theme: theme.success ? theme.data : DEFAULT_APPEARANCE.theme,
    accent: accent.success ? accent.data : DEFAULT_APPEARANCE.accent,
    customRoom: picked && isHex(picked.background) && isHex(picked.buttons) ? picked : DEFAULT_APPEARANCE.customRoom,
    font: font.success ? font.data : DEFAULT_APPEARANCE.font,
  };
}

export function writeMirror(look: Appearance) {
  try {
    localStorage.setItem(MIRROR_KEY, JSON.stringify({ theme: look.theme, accent: look.accent, customRoom: look.customRoom, font: look.font }));
  } catch {
    // ignore: the mirror only saves a flash on the next start
  }
}

/** Tests: back to an unthemed page with nothing remembered. */
export function resetAppearance() {
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.font;
  delete document.documentElement.dataset.accent;
  delete document.documentElement.dataset.room;
  for (const name of ROOM_PROPERTIES) document.documentElement.style.removeProperty(name);
  try {
    localStorage.removeItem(MIRROR_KEY);
  } catch {
    // ignore
  }
}
