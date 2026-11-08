import { screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { renderApp } from '../../test/backend';
import SkinStudio from './SkinStudio';
import css from './skins.css?raw';


/** The body of the first rule whose selector is exactly `selector`. */
function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = new RegExp(`(?:^|\\n)${escaped}\\s*\\{([^}]*)\\}`).exec(css);
  if (!match) throw new Error(`no rule for ${selector} in skins.css`);
  return match[1]!;
}

// jsdom lays nothing out, so these pin the rules that keep text whole at the 960 px minimum (checked in a real
// browser when they were written): Spanish labels run about a third longer than English ones.

it('wraps New and Import onto their own rows when the Wardrobe column is too narrow for both', async () => {
  renderApp(<SkinStudio />);
  const importKey = await screen.findByRole('button', { name: 'Import' });
  const row = importKey.parentElement!;
  expect(row).toHaveClass('wardrobe-actions');
  expect(row).toContainElement(screen.getByRole('button', { name: 'New' }));
  expect(rule('.wardrobe-actions')).toMatch(/flex-wrap:\s*wrap/);
  // A key never shrinks below its own label; a row with room shares it out.
  expect(rule('.wardrobe-actions > .btn')).toMatch(/flex:\s*1 0 auto/);
});

it("keeps a part's name whole on one line and lets the layer heads give way instead", () => {
  // The names' column is as wide as the longest name; the two layer columns take what is left.
  expect(rule('.parts-grid')).toMatch(/grid-template-columns:\s*max-content minmax\(0, 1fr\) minmax\(0, 1fr\)/);
  const name = rule('.parts-name');
  expect(name).toMatch(/white-space:\s*nowrap/);
  expect(name).not.toMatch(/overflow-wrap|text-overflow/);
  // A head too narrow for its eye and its label side by side puts the eye above the label.
  expect(rule('.parts-head')).toMatch(/flex-wrap:\s*wrap/);
});
