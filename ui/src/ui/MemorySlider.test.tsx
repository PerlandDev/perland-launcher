import { render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { MemorySlider } from './MemorySlider';

const slider = () => screen.getByRole('slider', { name: 'Memory' });

it('treats unknown system memory (0) as unknown: full range, no warning, no GB figure', () => {
  render(<MemorySlider label="Memory" valueMb={4096} onChange={vi.fn()} systemMb={0} />);
  expect(slider()).toHaveAttribute('max', '65536');
  expect(slider()).toHaveValue('4096');
  expect(screen.queryByText(/more than half/)).not.toBeInTheDocument();
  expect(screen.getByText(/couldn't read this computer's memory/)).toBeInTheDocument();
});

it('never caps the range below the current value', () => {
  render(<MemorySlider label="Memory" valueMb={16384} onChange={vi.fn()} systemMb={8192} />);
  expect(slider()).toHaveAttribute('max', '16384');
  expect(slider()).toHaveValue('16384');
});

it('reads out its value in GB and points at the hint', () => {
  render(<MemorySlider label="Memory" valueMb={2560} onChange={vi.fn()} systemMb={16384} />);
  expect(slider()).toHaveAttribute('aria-valuetext', '2.5 GB');
  expect(slider()).toHaveAccessibleDescription('This computer has 16 GB.');
});
