import { expect, it } from 'vitest';
import css from '../styles/app.css?raw';

/** The body of the first rule whose selector is exactly `selector`, at any indent. */
function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = new RegExp(`\\n\\s*${escaped}\\s*\\{([^}]*)\\}`).exec(css);
  if (!match) throw new Error(`no rule for ${selector} in app.css`);
  return match[1]!;
}

// jsdom lays nothing out and clips nothing, so these pin the rules (checked in a real browser).
it('shares the stepped circle with the whole app, so Home can use it too', () => {
  expect(rule(':root')).toMatch(/--pixel-circle:/);
});

it('backs every rail icon with a tile, so a picture with clear corners still reads as the chosen shape', () => {
  expect(rule('.rail-art')).toMatch(/background:\s*var\(--color-ground\)/);
});

it("rounds Home's icon plate and its picture when the icons are round", () => {
  expect(rule(".icon-plate[data-shape='round'] img")).toMatch(/clip-path:\s*var\(--pixel-circle\)/);
});

it("sizes Home's icon plate from the name's own type, so it never towers over a name the window shrinks", () => {
  const plate = rule('.icon-plate');
  expect(plate).toMatch(/--name-fit/);
  expect(plate).toMatch(/5\.2cqi/);
  expect(plate).not.toMatch(/width:\s*4\.5rem/);
});
