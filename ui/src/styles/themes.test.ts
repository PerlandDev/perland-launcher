import { describe, expect, it } from 'vitest';
import { THEMES } from '../state/appearance';
import css from './app.css?raw';

/** What a theme swaps: the room and its steps. The role hues (amber, lime, coral, blue) never change. */
const NEUTRALS = ['ground', 'panel', 'raised', 'line', 'ink', 'ink-dim', 'dirt', 'dirt-deep'] as const;

type Tokens = Record<string, string>;

/** `#abc` reads as `#aabbcc`, so a shorthand value is tested too. */
const longHex = (hex: string) => (hex.length === 4 ? `#${[...hex.slice(1)].map((c) => c + c).join('')}` : hex).toLowerCase();

function tokens(body: string): Tokens {
  return Object.fromEntries(
    [...body.matchAll(/--color-([a-z-]+):\s*(#[0-9a-fA-F]{6}|#[0-9a-fA-F]{3})\s*;/g)].map((m) => [m[1]!, longHex(m[2]!)]),
  );
}

/** The `@theme` block: the default room plus the shared role hues. */
function baseTokens(source: string): Tokens {
  const block = /@theme\s*\{([^}]*)\}/.exec(source);
  if (!block) throw new Error('no @theme block in app.css');
  return tokens(block[1]!);
}

/** Every `:root[data-theme='x']` block, by theme key. */
function themeBlocks(source: string): Record<string, Tokens> {
  const blocks: Record<string, Tokens> = {};
  for (const m of source.matchAll(/:root\[data-theme='([a-z]+)'\][^{]*\{([^}]*)\}/g)) blocks[m[1]!] = tokens(m[2]!);
  return blocks;
}

function luminance(hex: string) {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

function contrast(a: string, b: string) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi! + 0.05) / (lo! + 0.05);
}

/** Each failing pair, as "text on surface 3.21 < 4.5", so a red run says exactly what to fix. */
function contrastFailures(theme: Tokens): string[] {
  const failures: string[] = [];
  const need = (fg: string, bg: string, min: number) => {
    const ratio = contrast(theme[fg]!, theme[bg]!);
    if (!(ratio >= min)) failures.push(`${fg} on ${bg} ${ratio.toFixed(2)} < ${min}`);
  };
  for (const surface of ['ground', 'panel', 'raised']) for (const text of ['ink', 'ink-dim', 'blue-ink', 'coral']) need(text, surface, 4.5);
  // Lit button faces carry ground-coloured words.
  need('ground', 'amber', 4.5);
  need('ground', 'coral', 4.5);
  // The running lamp and the toggle knob are UI, not text.
  need('lime', 'ground', 3);
  return failures;
}

const base = baseTokens(css);
const blocks = themeBlocks(css);

it('has one block for every theme and none for anything else', () => {
  expect(Object.keys(blocks).sort()).toEqual([...THEMES].sort());
});

it('makes Laminate, the default, exactly the @theme room', () => {
  for (const key of NEUTRALS) expect(blocks.laminate?.[key], key).toBe(base[key]);
});

describe.each(THEMES.map((theme) => [theme]))('%s', (theme) => {
  it('overrides exactly the eight neutrals, never a role hue', () => {
    expect(Object.keys(blocks[theme]!).sort()).toEqual([...NEUTRALS].sort());
  });

  it('keeps every text and lamp readable', () => {
    expect(contrastFailures({ ...base, ...blocks[theme] })).toEqual([]);
  });
});

it('catches a theme that drops below the bar', () => {
  // The guard itself: a dim ink of #555 on Night's ground must fail, or the test above proves nothing.
  const bad: Tokens = { ...base, ...blocks.night, 'ink-dim': '#555555' };
  expect(contrastFailures(bad)).toContain(`ink-dim on ground ${contrast('#555555', bad.ground!).toFixed(2)} < 4.5`);
});

it('swaps the text font only through data-font', () => {
  expect(css).toMatch(/:root\[data-font='system'\]\s*\{[^}]*--font-sans:/);
});
