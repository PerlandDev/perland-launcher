import type { CustomRoom } from '../ipc/schema';

// The player's own room (spec 2026-11-25 §Your room): the whole room mixed from two colours, and the checks that keep it
// readable. The made rooms are held to the same floor by styles/themes.test.ts.

/** The shared role hues, the same in every room (app.css `@theme`). */
const LIME = '#c9cc2a';
const CORAL = '#ff5a5f';
const BLUE_INK = '#5cb3f0';
const WHITE = '#ffffff';

export const isHex = (value: string): boolean => /^#[0-9a-fA-F]{6}$/.test(value);

const channels = (hex: string): [number, number, number] => {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
};

const toHex = (c: readonly number[]) => '#' + c.map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');

/** `a` moved `t` (0 to 1) of the way to `b`. */
export function mix(a: string, b: string, t: number): string {
  const [x, y] = [channels(a), channels(b)];
  return toHex(x.map((v, i) => v + (y[i]! - v) * t));
}

export function luminance(hex: string): number {
  const [r, g, b] = channels(hex).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

/** The room's tokens, by `--color-<name>`: the eight neutrals and the act hue, as a made room swaps them. */
export type RoomTokens = Record<'ground' | 'panel' | 'raised' | 'line' | 'ink' | 'ink-dim' | 'dirt' | 'dirt-deep' | 'amber' | 'amber-hi', string>;

/** Every neutral is mixed from the two picks, so the room stays one family, the way the made rooms are. */
export function deriveRoom({ background, buttons }: CustomRoom): RoomTokens {
  const ink = mix(WHITE, background, 0.06);
  const dirt = mix(mix(background, WHITE, 0.16), buttons, 0.3);
  return {
    ground: background,
    panel: mix(background, WHITE, 0.035),
    raised: mix(background, WHITE, 0.07),
    line: mix(background, WHITE, 0.17),
    ink,
    'ink-dim': mix(ink, background, 0.3),
    dirt,
    'dirt-deep': mix(background, dirt, 0.5),
    amber: buttons,
    'amber-hi': mix(buttons, WHITE, 0.25),
  };
}

export type RoomCheckId = 'dark' | 'text' | 'play' | 'status';

/** What would break, each check passing or failing; the screen words them by `id`. */
export function roomChecks(room: CustomRoom): { id: RoomCheckId; ok: boolean }[] {
  if (!isHex(room.background) || !isHex(room.buttons)) {
    return (['dark', 'text', 'play', 'status'] as const).map((id) => ({ id, ok: false }));
  }
  const t = deriveRoom(room);
  return [
    { id: 'dark', ok: luminance(t.ground) <= 0.02 },
    { id: 'text', ok: contrast(t['ink-dim'], t.raised) >= 4.5 },
    { id: 'play', ok: contrast(t.ground, t.amber) >= 4.5 },
    { id: 'status', ok: contrast(LIME, t.ground) >= 3 && contrast(CORAL, t.raised) >= 4.5 && contrast(BLUE_INK, t.raised) >= 4.5 },
  ];
}

/** Whether the room may be put on: a room that fails a check is kept but not shown. */
export const roomOk = (room: CustomRoom): boolean => roomChecks(room).every((c) => c.ok);

/** A colour from hue (degrees), saturation and lightness (0 to 1). */
function hsl(h: number, sat: number, light: number): string {
  const k = (n: number) => (n + h / 30) % 12;
  const a = sat * Math.min(light, 1 - light);
  const f = (n: number) => light - a * Math.max(-1, Math.min(k(n) - 3, 9 - k(n), 1));
  return toHex([f(0), f(8), f(4)].map((v) => v * 255));
}

/** The hues the palettes walk, a colour wheel from red, by the name the screen gives each. */
const HUES = [
  ['red', 0],
  ['orange', 20],
  ['amber', 40],
  ['yellow', 60],
  ['lime', 90],
  ['green', 130],
  ['teal', 165],
  ['cyan', 190],
  ['sky', 210],
  ['blue', 240],
  ['violet', 270],
  ['pink', 310],
] as const;

/** One colour of a palette: its value and the hue and tone the screen names it by (`personalize.hues`, `.tones`). */
export interface Swatch {
  value: string;
  hue: string;
  tone: 'veryDark' | 'dark' | 'light' | 'mid' | 'bold';
}

/** The player's own room starts here (the default Rust saves, `settings::CustomRoom`). */
export const DEFAULT_ROOM: CustomRoom = { background: '#101820', buttons: '#ff6ad5' };

const passes = (background: string) => roomChecks({ background, buttons: WHITE }).every((c) => c.ok);

/** The lighter background of a hue: as light as the checks allow, up to 10 %. Greens and yellows stop sooner, since
 *  the lime lamp must still stand out on them. */
const lighter = (h: number): string => [0.1, 0.09, 0.085].map((l) => hsl(h, 0.3, l)).find(passes) ?? hsl(h, 0.3, 0.08);

const once = (list: Swatch[]) => list.filter((swatch, i) => list.findIndex((other) => other.value === swatch.value) === i);

/** Backgrounds a player can pick: two depths of every hue and of grey, each dark enough that text, Play and the role
 *  colours read on it, whatever the buttons (`roomChecks`). The default's own background leads. */
export const BACKGROUNDS: readonly Swatch[] = once([
  { value: DEFAULT_ROOM.background, hue: 'slate', tone: 'dark' },
  ...HUES.flatMap(([hue, h]): Swatch[] => [
    { value: hsl(h, 0.4, 0.07), hue, tone: 'veryDark' },
    { value: lighter(h), hue, tone: 'dark' },
  ]),
  { value: hsl(0, 0, 0.07), hue: 'grey', tone: 'veryDark' },
  { value: hsl(0, 0, 0.1), hue: 'grey', tone: 'dark' },
]).filter((swatch) => passes(swatch.value));

/** Buttons a player can pick: three tones of every hue, light to bold, the default's own and bone; `buttonsFor` keeps
 *  those Play's words read on. */
export const BUTTONS: readonly Swatch[] = once([
  ...HUES.flatMap(([hue, h]): Swatch[] => [
    { value: hsl(h, 0.9, 0.82), hue, tone: 'light' },
    { value: hsl(h, 0.85, 0.7), hue, tone: 'mid' },
    { value: hsl(h, 0.8, 0.6), hue, tone: 'bold' },
  ]),
  { value: DEFAULT_ROOM.buttons, hue: 'magenta', tone: 'bold' },
  { value: '#e8e0cc', hue: 'bone', tone: 'light' },
]);

/** The buttons that read on a background, in the palette's order. */
export const buttonsFor = (background: string): Swatch[] => BUTTONS.filter((swatch) => roomOk({ background, buttons: swatch.value }));

/** The buttons to keep on a new background: the same ones when they still read, else the nearest that do. */
export function fitButtons(background: string, buttons: string): string {
  const fit = buttonsFor(background).map((swatch) => swatch.value);
  if (fit.includes(buttons)) return buttons;
  const from = channels(isHex(buttons) ? buttons : DEFAULT_ROOM.buttons);
  const far = (b: string) => channels(b).reduce((sum, v, i) => sum + (v - from[i]!) ** 2, 0);
  return [...fit].sort((a, b) => far(a) - far(b))[0] ?? DEFAULT_ROOM.buttons;
}
