import { afterEach, expect, it, vi } from 'vitest';
import { applyAppearance, DEFAULT_APPEARANCE, readMirror, resetAppearance, writeMirror } from './appearance';

const KEY = 'perland.appearance';

afterEach(resetAppearance);

it('marks the page with the theme and font and remembers them for the next start', () => {
  applyAppearance({ theme: 'night', font: 'system' });
  expect(document.documentElement.dataset.theme).toBe('night');
  expect(document.documentElement.dataset.font).toBe('system');
  expect(readMirror()).toEqual({ theme: 'night', font: 'system' });

  applyAppearance({ theme: 'forest', font: 'hyperlegible' });
  expect(document.documentElement).toHaveAttribute('data-theme', 'forest');
  expect(document.documentElement).toHaveAttribute('data-font', 'hyperlegible');
  expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ theme: 'forest', font: 'hyperlegible' });
});

it('starts from the defaults when nothing is remembered', () => {
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  expect(DEFAULT_APPEARANCE).toEqual({ theme: 'laminate', font: 'hyperlegible' });
});

it('reads garbage as the defaults, keeping whichever half is valid', () => {
  localStorage.setItem(KEY, '{not json');
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  localStorage.setItem(KEY, JSON.stringify(['night']));
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  localStorage.setItem(KEY, 'null');
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  localStorage.setItem(KEY, JSON.stringify({ theme: 'neon', font: 'system' }));
  expect(readMirror()).toEqual({ theme: 'laminate', font: 'system' });
  localStorage.setItem(KEY, JSON.stringify({ theme: 'stone', font: 42 }));
  expect(readMirror()).toEqual({ theme: 'stone', font: 'hyperlegible' });
});

it('survives storage that throws', () => {
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
    throw new Error('denied');
  });
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('denied');
  });
  expect(readMirror()).toEqual(DEFAULT_APPEARANCE);
  expect(() => writeMirror({ theme: 'night', font: 'system' })).not.toThrow();
  // The page still changes even when it can't be remembered.
  applyAppearance({ theme: 'stone', font: 'system' });
  expect(document.documentElement.dataset.theme).toBe('stone');
});
