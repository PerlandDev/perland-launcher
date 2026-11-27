import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { Places } from './Places';

const prefetch = vi.hoisted(() => vi.fn());
vi.mock('../skins/loadSkinStudio', () => ({ prefetchSkinStudio: prefetch }));

beforeEach(() => {
  prefetch.mockReset();
});

it('starts the skin studio downloading as soon as the pointer is over Skins', () => {
  render(<Places />);
  fireEvent.pointerEnter(screen.getByRole('button', { name: 'Skins' }));
  expect(prefetch).toHaveBeenCalledTimes(1);
});

it('starts it when the keyboard reaches Skins too', () => {
  render(<Places />);
  fireEvent.focus(screen.getByRole('button', { name: 'Skins' }));
  expect(prefetch).toHaveBeenCalledTimes(1);
});

it('leaves the other places alone', () => {
  render(<Places />);
  for (const name of ['Servers', 'Mods & more', 'Medals and achievements']) {
    fireEvent.pointerEnter(screen.getByRole('button', { name }));
    fireEvent.focus(screen.getByRole('button', { name }));
  }
  expect(prefetch).not.toHaveBeenCalled();
});
