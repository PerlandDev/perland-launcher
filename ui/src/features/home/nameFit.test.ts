import { describe, expect, it } from 'vitest';
import { widestWordEm } from './nameFit';

describe('widestWordEm', () => {
  it('measures the widest word in Bungee, with a little room', () => {
    // ADVENTURES is 7.062 em in Bungee.
    expect(widestWordEm('Skyblock Adventures')).toBeCloseTo(7.062 * 1.02, 2);
  });

  it('gives wide letters their real width', () => {
    expect(widestWordEm('WWWWWWWWWW')).toBeGreaterThan(widestWordEm('IIIIIIIIII'));
    expect(widestWordEm('WWWWWWWWWW')).toBeCloseTo(8.31 * 1.02, 2);
  });

  it('ignores case and accents, as Bungee draws them on the same capitals', () => {
    expect(widestWordEm('difícil')).toBe(widestWordEm('DIFICIL'));
  });

  it('lets a hyphen end a word, as the browser breaks after it', () => {
    expect(widestWordEm('Night-Shift')).toBe(widestWordEm('Night-'));
  });

  it('counts an unknown character as the widest letter', () => {
    expect(widestWordEm('🌲🌲')).toBeCloseTo(2 * 0.849 * 1.02, 2);
  });

  it('is never less than one wide letter, so the size never divides by zero', () => {
    expect(widestWordEm('')).toBeCloseTo(0.849 * 1.02, 2);
  });
});
