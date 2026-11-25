import { screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { navigate } from './state/route';
import { renderApp } from './test/backend';
import App from './App';

// Settings stands in for any screen that breaks while it draws.
const broken = vi.hoisted(() => ({ settings: false }));
vi.mock('./features/settings/Settings', async (importOriginal) => {
  const real = await importOriginal<typeof import('./features/settings/Settings')>();
  return {
    ...real,
    Settings: () => {
      if (broken.settings) throw new Error('Cannot read properties of undefined');
      return <real.Settings />;
    },
  };
});

// The shell's idle moment runs at once here, so a test can see what it starts.
const idle = vi.hoisted(() => vi.fn((work: () => void) => (work(), () => {})));
vi.mock('./ui/whenIdle', () => ({ whenIdle: idle }));
const prefetch = vi.hoisted(() => vi.fn());
vi.mock('./features/skins/loadSkinStudio', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./features/skins/loadSkinStudio')>()),
  prefetchSkinStudio: prefetch,
}));

beforeEach(() => {
  broken.settings = false;
  idle.mockClear();
  prefetch.mockClear();
  vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  vi.restoreAllMocks();
});

it('shows an error card with Go home when a screen breaks, instead of a blank window', async () => {
  broken.settings = true;
  const { user } = renderApp(<App />);
  await screen.findByRole('heading', { name: 'Asalith Fields' });
  navigate({ name: 'settings' });
  const alert = await screen.findByRole('alert');
  expect(alert).toHaveTextContent('This screen ran into a problem.');
  expect(alert).toHaveTextContent('Go home and try again. If it keeps happening, restart the launcher.');
  // The rest of the window still works: the title bar and the rail are there.
  expect(screen.getByRole('navigation', { name: 'Places' })).toBeInTheDocument();
  expect(screen.getByRole('navigation', { name: 'Instances' })).toBeInTheDocument();
  broken.settings = false;
  await user.click(within(alert).getByRole('button', { name: 'Go home' }));
  expect(await screen.findByRole('heading', { name: 'Asalith Fields' })).toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('lets go of a broken screen when the player moves elsewhere from the title bar or the rail', async () => {
  broken.settings = true;
  const { user } = renderApp(<App />);
  await screen.findByRole('heading', { name: 'Asalith Fields' });
  navigate({ name: 'settings' });
  await screen.findByRole('alert');
  await user.click(screen.getByRole('button', { name: 'Servers' }));
  expect(await screen.findByRole('heading', { name: 'Servers', level: 1 })).toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('downloads the skin studio in an idle moment once the shell is up', async () => {
  renderApp(<App />);
  await screen.findByRole('heading', { name: 'Asalith Fields' });
  expect(idle).toHaveBeenCalledTimes(1);
  expect(prefetch).toHaveBeenCalledTimes(1);
});

it('downloads nothing ahead during first run', async () => {
  renderApp(<App />, { firstRun: true });
  await screen.findByRole('heading', { name: 'Welcome to Perland' });
  expect(prefetch).not.toHaveBeenCalled();
});
