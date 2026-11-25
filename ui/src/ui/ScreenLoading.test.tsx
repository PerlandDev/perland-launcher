import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { ScreenLoading, SHOW_AFTER_MS, SLOW_AFTER_MS } from './ScreenLoading';

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

it('shows nothing at first, so a fast load never flickers', () => {
  const { container } = render(<ScreenLoading label="Setting up the skin studio…" />);
  // The region is there and busy from the start, but empty.
  expect(container.querySelector('[aria-busy="true"]')).toBeInTheDocument();
  expect(screen.queryByText('Setting up the skin studio…')).not.toBeInTheDocument();
  expect(container.querySelector('.meter')).not.toBeInTheDocument();
});

it('then shows the sweeping meter over the words, as a status', () => {
  const { container } = render(<ScreenLoading label="Setting up the skin studio…" />);
  act(() => void vi.advanceTimersByTime(SHOW_AFTER_MS));
  expect(screen.getByRole('status')).toHaveTextContent('Setting up the skin studio…');
  const meter = container.querySelector('.meter')!;
  // Play's checking meter: a recessed channel with one block sweeping it, decorative because the words say it.
  expect(meter).toHaveAttribute('data-scan');
  expect(meter).toHaveAttribute('data-channel');
  expect(meter).toHaveAttribute('aria-hidden', 'true');
  expect(screen.queryByText('This is taking longer than usual.')).not.toBeInTheDocument();
  expect(screen.queryByRole('button')).not.toBeInTheDocument();
});

it('says when it is taking longer than usual, and offers to try again', () => {
  const retry = vi.fn();
  render(<ScreenLoading label="Getting your settings…" onRetry={retry} />);
  act(() => void vi.advanceTimersByTime(SLOW_AFTER_MS));
  expect(screen.getByRole('status')).toHaveTextContent('This is taking longer than usual.');
  fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
  expect(retry).toHaveBeenCalledTimes(1);
});

it('starts its clock again after a try, so the slow line waits its turn', () => {
  const retry = vi.fn();
  render(<ScreenLoading label="Getting your settings…" onRetry={retry} />);
  act(() => void vi.advanceTimersByTime(SLOW_AFTER_MS));
  fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
  expect(screen.queryByText('This is taking longer than usual.')).not.toBeInTheDocument();
  // The words stay: the screen is still on its way.
  expect(screen.getByRole('status')).toHaveTextContent('Getting your settings…');
  act(() => void vi.advanceTimersByTime(SLOW_AFTER_MS));
  expect(screen.getByText('This is taking longer than usual.')).toBeInTheDocument();
});
