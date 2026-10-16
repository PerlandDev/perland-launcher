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
