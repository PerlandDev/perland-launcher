import { describe, expect, it } from 'vitest';
import { BACKGROUNDS, BUTTONS, DEFAULT_ROOM, buttonsFor, type Swatch, contrast, deriveRoom, fitButtons, isHex, mix, roomChecks, roomOk } from './customRoom';

describe('mix', () => {
  it('moves one colour toward another', () => {
    expect(mix('#000000', '#ffffff', 0.5)).toBe('#808080');
    expect(mix('#102030', '#102030', 0.3)).toBe('#102030');
    expect(mix('#ff0000', '#0000ff', 0)).toBe('#ff0000');
  });
});

describe('isHex', () => {
  it('takes only #rrggbb', () => {
    expect(isHex('#a1B2c3')).toBe(true);
    for (const bad of ['a1b2c3', '#abc', '#12345g', '', '#1234567']) expect(isHex(bad), bad).toBe(false);
  });
});

describe('deriveRoom', () => {
  it('mixes the whole room from the background and the buttons', () => {
    const room = deriveRoom({ background: '#101820', buttons: '#ff6ad5' });
    expect(Object.keys(room).sort()).toEqual(['amber', 'amber-hi', 'dirt', 'dirt-deep', 'ground', 'ink', 'ink-dim', 'line', 'panel', 'raised']);
    expect(room.ground).toBe('#101820');
    expect(room.amber).toBe('#ff6ad5');
    // Surfaces step up from the floor, lighter each time.
    expect(contrast(room.panel, '#000000')).toBeGreaterThan(contrast(room.ground, '#000000'));
    expect(contrast(room.raised, '#000000')).toBeGreaterThan(contrast(room.panel, '#000000'));
    expect(contrast(room.line, '#000000')).toBeGreaterThan(contrast(room.raised, '#000000'));
  });
});

describe('roomChecks', () => {
  it('passes a dark room with light buttons', () => {
    const checks = roomChecks({ background: '#101820', buttons: '#ff6ad5' });
    expect(checks.map((c) => [c.id, c.ok])).toEqual([
      ['dark', true],
      ['text', true],
      ['play', true],
      ['status', true],
    ]);
    expect(roomOk({ background: '#101820', buttons: '#ff6ad5' })).toBe(true);
  });

  it('fails a light background', () => {
    const checks = roomChecks({ background: '#d8d0c0', buttons: '#ff7a2f' });
    expect(checks.find((c) => c.id === 'dark')?.ok).toBe(false);
    expect(roomOk({ background: '#d8d0c0', buttons: '#ff7a2f' })).toBe(false);
  });

  it("fails buttons too dark for Play's words", () => {
    expect(roomChecks({ background: '#101820', buttons: '#30204a' }).find((c) => c.id === 'play')?.ok).toBe(false);
  });

  it('fails a malformed colour', () => {
    expect(roomOk({ background: 'red', buttons: '#ff6ad5' })).toBe(false);
  });
});

describe('the palettes', () => {
  const values = (list: readonly Swatch[]) => list.map((swatch) => swatch.value);

  it('offers only backgrounds that pass every check that depends on the background', () => {
    expect(BACKGROUNDS.length).toBeGreaterThanOrEqual(24);
    expect(new Set(values(BACKGROUNDS)).size).toBe(BACKGROUNDS.length);
    for (const { value } of BACKGROUNDS) {
      expect(roomChecks({ background: value, buttons: '#ffffff' }).filter((c) => !c.ok), value).toEqual([]);
    }
  });

  it('offers, for every background, only buttons that pass every check, and plenty of them', () => {
    for (const { value: background } of BACKGROUNDS) {
      const buttons = buttonsFor(background);
      expect(buttons.length, background).toBeGreaterThanOrEqual(20);
      for (const { value } of buttons) expect(roomOk({ background, buttons: value }), `${background} ${value}`).toBe(true);
    }
  });

  it('keeps the buttons in one order whatever the background, so they do not jump around', () => {
    for (const { value: background } of BACKGROUNDS) {
      const fit = buttonsFor(background);
      expect(fit).toEqual(BUTTONS.filter((swatch) => fit.includes(swatch)));
    }
  });

  it('names every colour by a hue and a tone, and no two alike', () => {
    for (const list of [BACKGROUNDS, BUTTONS]) {
      const names = list.map((swatch) => `${swatch.hue} ${swatch.tone}`);
      expect(new Set(names).size).toBe(names.length);
    }
  });

  it('starts from a room the palettes hold', () => {
    expect(values(BACKGROUNDS)).toContain(DEFAULT_ROOM.background);
    expect(values(buttonsFor(DEFAULT_ROOM.background))).toContain(DEFAULT_ROOM.buttons);
  });

  it('keeps the chosen buttons on a new background when they still fit, else picks the closest that does', () => {
    const background = BACKGROUNDS[1]!.value;
    const fits = buttonsFor(background)[0]!.value;
    expect(fitButtons(background, fits)).toBe(fits);
    expect(values(buttonsFor(background))).toContain(fitButtons(background, '#202020'));
  });
});
