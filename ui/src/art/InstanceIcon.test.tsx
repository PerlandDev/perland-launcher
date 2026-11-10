import { render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { InstanceIcon } from './InstanceIcon';
import { PlotIcon } from './PlotIcon';
import { PLOT_HUES } from './random';
import { hueIndex } from './variant';

it('shows the initials on the hue of its seed', () => {
  const { container } = render(<InstanceIcon name="Asalith Fields" seed="asalith-fields" size={40} />);
  expect(screen.getAllByText('AF').length).toBeGreaterThan(0);
  const hue = PLOT_HUES[hueIndex('asalith-fields')]!;
  expect(container.querySelector(`[fill="${hue}"]`)).not.toBeNull();
});

it('is hidden from screen readers', () => {
  const { container } = render(<InstanceIcon name="Creative Flat" seed="creative-flat" />);
  expect(container.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
});

it('falls back to the plot icon when the name has no letter or digit', () => {
  const { container } = render(<InstanceIcon name="!!!" seed="bangs" size={40} />);
  const plot = render(<PlotIcon seed="bangs" size={40} />).container.innerHTML;
  expect(container.innerHTML).toBe(plot);
  expect(container.querySelector('text')).toBeNull();
});

it("paints the initials' plate, grass and dirt in the theme's own colours", async () => {
  const { PALETTES } = await import('./palette');
  try {
    document.documentElement.dataset.theme = 'night';
    const { container } = render(<InstanceIcon name="Asalith Fields" seed="asalith-fields" size={40} />);
    const palette = PALETTES.night;
    expect(container.querySelector(`[fill="${palette.hues[hueIndex('asalith-fields')]}"]`)).not.toBeNull();
    expect(container.querySelector(`[fill="${palette.dirt}"]`)).not.toBeNull();
    // Nothing of Laminate's warm palette is left.
    expect(container.querySelector(`[fill="${PLOT_HUES[hueIndex('asalith-fields')]}"]`)).toBeNull();
  } finally {
    delete document.documentElement.dataset.theme;
  }
});

it("paints the plot icon in the theme's own colours too", async () => {
  const { PALETTES } = await import('./palette');
  try {
    document.documentElement.dataset.theme = 'forest';
    const { container } = render(<PlotIcon seed="bangs" size={40} />);
    expect(container.querySelector(`[fill="${PALETTES.forest.hues[hueIndex('bangs')]}"]`)).not.toBeNull();
    expect(container.querySelector(`[fill="${PALETTES.forest.dirt}"]`)).not.toBeNull();
  } finally {
    delete document.documentElement.dataset.theme;
  }
});
