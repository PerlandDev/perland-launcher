import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { toastsSnapshot } from '../state/toasts';
import { copyText } from './copyText';

describe('copyText', () => {
  // user-event installs the clipboard stub on `navigator`.
  beforeEach(() => void userEvent.setup());

  it('puts the text on the clipboard', async () => {
    expect(await copyText('play.example.com:25577')).toBe(true);
    expect(await navigator.clipboard.readText()).toBe('play.example.com:25577');
    expect(toastsSnapshot()).toEqual([]);
  });

  it('reports a clipboard that refuses through the toast path', async () => {
    const failure = new Error('denied');
    vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValueOnce(failure);
    expect(await copyText('x')).toBe(false);
    expect(toastsSnapshot().map((t) => t.error)).toEqual([failure]);
  });
});
