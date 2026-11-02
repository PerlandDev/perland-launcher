import { expect, it } from 'vitest';
import { windowDraft, windowOf } from './gameWindow';

it('starts a custom size from 1280 × 720 and keeps a saved one', () => {
  expect(windowDraft({ mode: 'default' })).toEqual({ mode: 'default', width: '1280', height: '720' });
  expect(windowDraft({ mode: 'size', width: 1920, height: 1080 })).toEqual({ mode: 'size', width: '1920', height: '1080' });
});

it('turns the typed size into a window only within the limits', () => {
  const size = (width: string, height: string) => windowOf({ mode: 'size', width, height });
  expect(size('320', '240')).toEqual({ mode: 'size', width: 320, height: 240 });
  expect(size('16384', '16384')).toEqual({ mode: 'size', width: 16384, height: 16384 });
  for (const [w, h] of [['319', '720'], ['1280', '239'], ['16385', '720'], ['', '720'], ['12.5', '720'], ['-800', '600'], ['1e3', '720']] as const) {
    expect(size(w, h)).toBeNull();
  }
  // The other modes carry no size, whatever the fields hold.
  expect(windowOf({ mode: 'fullscreen', width: '', height: '' })).toEqual({ mode: 'fullscreen' });
  expect(windowOf({ mode: 'default', width: '1', height: '1' })).toEqual({ mode: 'default' });
});
