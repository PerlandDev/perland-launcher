import { useSyncExternalStore } from 'react';
import type { Theme } from '../ipc/schema';
import { DIRT, GROUND, PLOT_HUES } from './random';

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
export const PALETTES: Record<Theme, ArtPalette> = {
  laminate: { ground: GROUND, dirt: DIRT, lamp: '#ffd98a', hues: PLOT_HUES, sun: '#fff8e8' },
  // Deepslate: cool daylight over grey stone, with its ores for hues (diamond, iron-blue, amethyst, gold).
  stone: { ground: '#141619', dirt: '#5b636d', lamp: '#d4e4ee', hues: ['#2ec4b6', '#7fa8d6', '#b48cf0', '#e3c35a'], sun: '#f4fbff' },
  // Night sky: moonlight, with periwinkle, sky blue, violet and starlight gold.
  night: { ground: '#10131f', dirt: '#4a4f8a', lamp: '#c4ccff', hues: ['#8b95ff', '#5cb3f0', '#c792ff', '#ffe08a'], sun: '#f2f4ff' },
  // Moss: dappled sun, with leaf green, moss, autumn amber and a stream's teal.
  forest: { ground: '#111712', dirt: '#4f6b3a', lamp: '#e9f0b4', hues: ['#2fc46e', '#c9cc2a', '#e3a94f', '#4fb3a6'], sun: '#fbfff0' },
};

const isTheme = (value: string | undefined): value is Theme => value !== undefined && value in PALETTES;

/** The theme on screen now: `applyAppearance` keeps it on `<html data-theme>`. Unset (a test, the first paint) is Laminate. */
function currentTheme(): Theme {
  const theme = document.documentElement.dataset.theme;
  return isTheme(theme) ? theme : 'laminate';
}

function subscribe(onChange: () => void) {
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  return () => observer.disconnect();
}

/** The landscape palette of the theme on screen, following a theme change at once. */
export function useArtPalette(): ArtPalette {
  return PALETTES[useSyncExternalStore(subscribe, currentTheme, () => 'laminate' as Theme)];
}
