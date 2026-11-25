import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { ScreenBoundary } from './ScreenBoundary';
import { screenError } from './screenError';

// React reports every caught error on the console; the boundary's job is what the player sees instead.
beforeEach(() => {
  vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  vi.restoreAllMocks();
});

let broken = true;
function Fragile() {
  if (broken) throw new Error('Failed to fetch dynamically imported module: /assets/SkinStudio-abc123.js');
  return <p>Studio</p>;
}

it('shows the fallback instead of an empty window when a screen breaks, and can put the screen back', () => {
  broken = true;
  render(
    <ScreenBoundary fallback={(_, reset) => <button onClick={reset}>Again</button>}>
      <Fragile />
    </ScreenBoundary>,
  );
  expect(screen.queryByText('Studio')).not.toBeInTheDocument();
  broken = false;
  fireEvent.click(screen.getByRole('button', { name: 'Again' }));
  expect(screen.getByText('Studio')).toBeInTheDocument();
});

it('hands the fallback what went wrong', () => {
  broken = true;
  render(<ScreenBoundary fallback={(error) => <p>{(error as Error).message}</p>}>{<Fragile />}</ScreenBoundary>);
  expect(screen.getByText(/Failed to fetch dynamically imported module/)).toBeInTheDocument();
});

it('lets go of a failure when the player moves somewhere else', () => {
  broken = true;
  function Host() {
    const [place, setPlace] = useState('settings');
    return (
      <>
        <button onClick={() => setPlace('home')}>Home</button>
        <ScreenBoundary resetKey={place} fallback={() => <p>Broke</p>}>
          {place === 'settings' ? <Fragile /> : <p>Home screen</p>}
        </ScreenBoundary>
      </>
    );
  }
  render(<Host />);
  expect(screen.getByText('Broke')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Home' }));
  expect(screen.getByText('Home screen')).toBeInTheDocument();
});

it('words a caught failure as a launcher error, translated by its code, with the cause as the detail', () => {
  const error = screenError('skins_open_failed', new Error('Failed to fetch dynamically imported module'));
  expect(error).toMatchObject({ code: 'skins_open_failed', detail: 'Failed to fetch dynamically imported module', params: {} });
});
