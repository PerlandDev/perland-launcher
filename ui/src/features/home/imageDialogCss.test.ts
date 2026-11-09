import { expect, it } from 'vitest';
import css from '../../styles/app.css?raw';

/** The body of the first rule whose selector is exactly `selector`, at any indent. */
function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = new RegExp(`\\n\\s*${escaped}\\s*\\{([^}]*)\\}`).exec(css);
  if (!match) throw new Error(`no rule for ${selector} in app.css`);
  return match[1]!;
}

// jsdom lays nothing out, so these pin the single-column layout (checked in a real browser).
it('lays Change image out in one column: three icon keys to a row, the band across the whole dialog', () => {
  expect(rule('.art-keys-row')).toMatch(/grid-template-columns:\s*repeat\(3, minmax\(0, 1fr\)\)/);
  expect(rule('.art-band')).toMatch(/margin-inline:\s*-1\.5rem/);
  expect(css).not.toMatch(/\.art-sections\s*\{/);
});
