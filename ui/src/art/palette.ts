import { useSyncExternalStore } from 'react';
import type { CustomRoom, Theme } from '../ipc/schema';
import { deriveRoom, isHex, mix } from '../state/customRoom';
import { DIRT, GROUND, PLOT_HUES } from './random';

/** A made room: every theme but the player's own, which is mixed at runtime. */
type MadeTheme = Exclude<Theme, 'custom'>;

/** What a generated landscape is painted with: the room's dark floor and its dirt, the lamp behind the horizon, the
 *  four hues its rims and glow lean toward, and the near-white of its sun. */
export interface ArtPalette {
  ground: string;
  dirt: string;
  lamp: string;
  hues: readonly [string, string, string, string];
  sun: string;
}

/** One landscape palette per theme, so the default landscapes are the same hills and suns in every room, lit in that
 *  room's own colours. Laminate is the palette they always had: the mark's four plot hues under a warm tube light. */
export const PALETTES: Record<MadeTheme, ArtPalette> = {
  laminate: { ground: GROUND, dirt: DIRT, lamp: '#ffd98a', hues: PLOT_HUES, sun: '#fff8e8' },
  // Deepslate: cool daylight over grey stone, with its ores for hues (diamond, iron-blue, amethyst, gold).
  stone: { ground: '#141619', dirt: '#5b636d', lamp: '#d4e4ee', hues: ['#2ec4b6', '#7fa8d6', '#b48cf0', '#e3c35a'], sun: '#f4fbff' },
  // Night sky: moonlight, with periwinkle, sky blue, violet and starlight gold.
  night: { ground: '#10131f', dirt: '#4a4f8a', lamp: '#c4ccff', hues: ['#8b95ff', '#5cb3f0', '#c792ff', '#ffe08a'], sun: '#f2f4ff' },
  // Moss: dappled sun, with leaf green, moss, autumn amber and a stream's teal.
  forest: { ground: '#111712', dirt: '#4f6b3a', lamp: '#e9f0b4', hues: ['#2fc46e', '#c9cc2a', '#e3a94f', '#4fb3a6'], sun: '#fbfff0' },
  // Netherrack and its fires: ember orange, crimson, warped teal and glowstone.
  nether: { ground: '#1a0f0e', dirt: '#6e2b22', lamp: '#ffc08a', hues: ['#ff8c2e', '#e04a3c', '#3aa89b', '#ffd36b'], sun: '#fff3e6' },
  // The void over end stone: pale chorus, void violet, portal teal and end-stone sand.
  end: { ground: '#121019', dirt: '#4f416d', lamp: '#cfc8ff', hues: ['#e6dfa2', '#a78bfa', '#4fd1c5', '#d8d0a0'], sun: '#f6f3ff' },
  // Cherry grove at dusk: blossom pink, petal, cherry wood and leaf green.
  cherry: { ground: '#1b1216', dirt: '#6f3a53', lamp: '#ffc8dd', hues: ['#ff8fc0', '#ffd1e3', '#b5695a', '#7fc98a'], sun: '#fff4f8' },
};

/** The player's own room's landscapes: the same scene, lit by the two colours they picked, so the room and its
 *  pictures are one family (spec 2026-11-25). */
export function customPalette(room: CustomRoom): ArtPalette {
  const tokens = deriveRoom(room);
  const act = room.buttons;
  return {
    ground: tokens.ground,
    dirt: tokens.dirt,
    lamp: mix(act, '#ffffff', 0.4),
    hues: [act, mix(act, '#ffffff', 0.45), mix(act, tokens.dirt, 0.5), mix(act, tokens.ink, 0.35)],
    sun: mix(act, '#ffffff', 0.85),
  };
}

/** The two colours of the player's own room, as `applyAppearance` writes them on `<html data-room>`. */
function roomOnPage(): CustomRoom | null {
  const [background, buttons] = (document.documentElement.dataset.room ?? '').split(',');
  return background && buttons && isHex(background) && isHex(buttons) ? { background, buttons } : null;
}

/** The landscape palette of the room on screen now. An own room the page doesn't carry falls back to Laminate. */
export function currentPalette(): ArtPalette {
  const theme = document.documentElement.dataset.theme;
  if (theme === 'custom') {
    const room = roomOnPage();
    return room ? customPalette(room) : PALETTES.laminate;
  }
  return isTheme(theme) ? PALETTES[theme] : PALETTES.laminate;
}

const isTheme = (value: string | undefined): value is MadeTheme => value !== undefined && value in PALETTES;

function subscribe(onChange: () => void) {
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'data-room'] });
  return () => observer.disconnect();
}

/** What `<html>` says the look is, as one string, so React only re-renders when it really changes. */
const paletteKey = () => `${document.documentElement.dataset.theme ?? ''}|${document.documentElement.dataset.room ?? ''}`;

/** The landscape palette of the room on screen, following a change at once. */
export function useArtPalette(): ArtPalette {
  useSyncExternalStore(subscribe, paletteKey, () => '');
  return currentPalette();
}
