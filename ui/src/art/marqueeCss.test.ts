import { expect, it } from 'vitest';
import css from '../styles/app.css?raw';

/** The body of the first rule whose selector is exactly `selector`, at any indent. */
function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = new RegExp(`\\n\\s*${escaped}\\s*\\{([^}]*)\\}`).exec(css);
  if (!match) throw new Error(`no rule for ${selector} in app.css`);
  return match[1]!;
}

// jsdom lays nothing out, so this pins the rule (checked in a real browser): Tailwind's preflight caps every img at
// max-width: 100%, which squeezed a zoomed banner wider than the panel while its height stayed, stretching it.
it('lets a zoomed banner be wider and taller than the panel, so it is never stretched', () => {
  const zoomed = rule('.marquee-banner[data-zoomed]');
  expect(zoomed).toMatch(/max-width:\s*none/);
  expect(zoomed).toMatch(/max-height:\s*none/);
});
