import { theme as themeSchema, uiFont, type Theme, type UiFont } from '../ipc/schema';

export interface Appearance {
  theme: Theme;
  font: UiFont;
}

/** Every theme, in the order Settings offers them. The first is the default. */
export const THEMES = themeSchema.options;
export const FONTS = uiFont.options;

export const DEFAULT_APPEARANCE: Appearance = { theme: 'laminate', font: 'hyperlegible' };

/** A copy of the last applied look, read before the first render so a start never flashes the old theme. */
const MIRROR_KEY = 'perland.appearance';

/** Theme the whole page: app.css swaps the neutral tokens on `:root[data-theme]` and the body face on `:root[data-font]`. */
export function applyAppearance(look: Appearance) {
  const root = document.documentElement;
  root.dataset.theme = look.theme;
  root.dataset.font = look.font;
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
  const font = uiFont.safeParse(record.font);
  return {
    theme: theme.success ? theme.data : DEFAULT_APPEARANCE.theme,
    font: font.success ? font.data : DEFAULT_APPEARANCE.font,
  };
}

export function writeMirror(look: Appearance) {
  try {
    localStorage.setItem(MIRROR_KEY, JSON.stringify({ theme: look.theme, font: look.font }));
  } catch {
    // ignore: the mirror only saves a flash on the next start
  }
}

/** Tests: back to an unthemed page with nothing remembered. */
export function resetAppearance() {
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.font;
  try {
    localStorage.removeItem(MIRROR_KEY);
  } catch {
    // ignore
  }
}
