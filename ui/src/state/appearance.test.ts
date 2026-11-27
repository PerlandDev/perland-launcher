import { afterEach, expect, it, vi } from 'vitest';
import { applyAppearance, DEFAULT_APPEARANCE, readMirror, resetAppearance, writeMirror } from './appearance';

const LOOK = { theme: 'night', accent: 'room', customRoom: { background: '#101820', buttons: '#ff6ad5' }, font: 'system' } as const;

const KEY = 'perland.appearance';

afterEach(resetAppearance);

it('marks the page with the theme and font and remembers them for the next start', () => {
  applyAppearance(LOOK);
  expect(document.documentElement.dataset.theme).toBe('night');
  expect(document.documentElement.dataset.font).toBe('system');
  expect(readMirror()).toEqual(LOOK);

  applyAppearance({ ...LOOK, theme: 'forest', font: 'hyperlegible' });
  expect(document.documentElement).toHaveAttribute('data-theme', 'forest');
  expect(document.documentElement).toHaveAttribute('data-font', 'hyperlegible');
  expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ ...LOOK, theme: 'forest', font: 'hyperlegible' });
});

it("marks the page with a button colour, but never with the room's own", () => {
  applyAppearance({ ...LOOK, accent: 'violet' });
  expect(document.documentElement.dataset.accent).toBe('violet');
  applyAppearance({ ...LOOK, accent: 'room' });
  expect(document.documentElement).not.toHaveAttribute('data-accent');
});

it("puts the player's own room on the page, and keeps a room that fails a check off it", () => {
  applyAppearance({ ...LOOK, theme: 'custom' });
  expect(document.documentElement.dataset.theme).toBe('custom');
  expect(document.documentElement.dataset.room).toBe('#101820,#ff6ad5');
  expect(document.documentElement.style.getPropertyValue('--color-ground')).toBe('#101820');
  expect(document.documentElement.style.getPropertyValue('--color-amber')).toBe('#ff6ad5');
  // A button colour never overrides the room the player mixed themselves.
  expect(document.documentElement).not.toHaveAttribute('data-accent');

  // Too light to read on: the room is still remembered, but Laminate is what shows.
  applyAppearance({ ...LOOK, theme: 'custom', customRoom: { background: '#d8d0c0', buttons: '#ff7a2f' } });
  expect(document.documentElement.dataset.theme).toBe('laminate');
  expect(document.documentElement).not.toHaveAttribute('data-room');
  expect(document.documentElement.style.getPropertyValue('--color-ground')).toBe('');
  expect(readMirror().customRoom).toEqual({ background: '#d8d0c0', buttons: '#ff7a2f' });

  // Leaving the own room takes its colours off the page.
  applyAppearance({ ...LOOK, theme: 'custom' });
  applyAppearance({ ...LOOK, theme: 'stone' });
  expect(document.documentElement).not.toHaveAttribute('data-room');
  expect(document.documentElement.style.getPropertyValue('--color-ground')).toBe('');
});

it('starts from the defaults when nothing is remembered', () => {
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  expect(DEFAULT_APPEARANCE).toEqual({ theme: 'laminate', accent: 'room', customRoom: { background: '#101820', buttons: '#ff6ad5' }, font: 'hyperlegible' });
});

it('reads garbage as the defaults, keeping whichever half is valid', () => {
  localStorage.setItem(KEY, '{not json');
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  localStorage.setItem(KEY, JSON.stringify(['night']));
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  localStorage.setItem(KEY, 'null');
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  localStorage.setItem(KEY, JSON.stringify({ theme: 'neon', font: 'system' }));
  expect(readMirror()).toEqual({ ...DEFAULT_APPEARANCE, font: 'system' });
  localStorage.setItem(KEY, JSON.stringify({ theme: 'stone', font: 42 }));
  expect(readMirror()).toEqual({ ...DEFAULT_APPEARANCE, theme: 'stone' });
  localStorage.setItem(KEY, JSON.stringify({ accent: 'neon', customRoom: { background: 'red', buttons: '#ff6ad5' } }));
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  localStorage.setItem(KEY, JSON.stringify({ accent: 'cyan', customRoom: { background: '#0F1C26', buttons: '#5ad1e6' } }));
  expect(readMirror()).toEqual({ ...DEFAULT_APPEARANCE, accent: 'cyan', customRoom: { background: '#0f1c26', buttons: '#5ad1e6' } });
});

it('survives storage that throws', () => {
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
    throw new Error('denied');
  });
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('denied');
  });
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  expect(() => writeMirror(LOOK)).not.toThrow();
  // The page still changes even when it can't be remembered.
  applyAppearance({ ...LOOK, theme: 'stone' });
  expect(document.documentElement.dataset.theme).toBe('stone');
});
