// The hotbar's slots and the colour presets, shared by the screen and its panels.
import type { Tool } from './paint';

/** The nine slots: six tools, then Mirror, Undo and Redo. Number keys 1–9 press them. */
export const SLOT_TOOLS: Tool[] = ['pencil', 'eraser', 'fill', 'picker', 'shade', 'noise'];

/** Skin tones, hair and cloth: a starting row, not a limit. */
export const PRESETS = [
  '#f3d2b3', '#e0ac85', '#c98e6a', '#a26a48', '#7a4b30', '#4a2c1c',
  '#2b1d14', '#6b4024', '#d9a441', '#f0e2b6', '#1f1f24', '#f6f1e7',
  '#c0392b', '#e67e22', '#6cab3c', '#2d7d46', '#3a6fd1', '#7b4bb3',
];
