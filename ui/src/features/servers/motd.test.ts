import { describe, expect, it } from 'vitest';
import { motdPlain, motdRuns } from './motd';

describe('motdRuns', () => {
  it('splits § colour and format codes into runs', () => {
    expect(motdRuns('§6Survival§r with §lfriends')).toEqual([
      { text: 'Survival', color: '6', bold: false, italic: false, underline: false, strike: false },
      { text: ' with ', color: null, bold: false, italic: false, underline: false, strike: false },
      { text: 'friends', color: null, bold: true, italic: false, underline: false, strike: false },
    ]);
  });

  it('drops obfuscation, unknown codes and a trailing §, and a colour resets formats', () => {
    expect(motdRuns('§l§kx§zy§az§').map((r) => [r.text, r.color, r.bold])).toEqual([
      ['x', null, true],
      ['y', null, true],
      ['z', 'a', false],
    ]);
    expect(motdPlain('§6Survival§r\nwith friends')).toBe('Survival\nwith friends');
  });
});
