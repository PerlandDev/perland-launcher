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
