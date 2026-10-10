import { emit } from '@tauri-apps/api/event';
import { act, screen, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { CLOSE_REQUESTED } from './ipc/events';
import { createMockBackend } from './ipc/mock';
import { renderApp, sent } from './test/backend';
import { renderWithProviders } from './test/render';
import App from './App';

describe('App', () => {
  it('starts with the first-run flow on a fresh install', async () => {
    renderApp(<App />, { firstRun: true });
    expect(await screen.findByRole('heading', { name: 'Welcome to Perland' })).toBeInTheDocument();
  });

  it('shows the rail and the selected instance otherwise', async () => {
    renderApp(<App />);
    expect(await screen.findByRole('navigation', { name: 'Instances' })).toBeInTheDocument();
    expect(await screen.findByRole('heading', { name: 'Asalith Fields' })).toBeInTheDocument();
  });

  it('asks before quitting while a game runs', async () => {
    const { user, calls } = renderApp(<App />);
    await screen.findByRole('heading', { name: 'Asalith Fields' });
    await act(() => emit(CLOSE_REQUESTED));
    const dialog = await screen.findByRole('dialog', { name: 'A game is still running' });
    expect(dialog).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Quit and stop games' }));
    expect(sent(calls, 'quit')).toEqual([{ stopGames: true }]);
  });

  it('does not fall through to the shell when the settings cannot be loaded, and can try again', async () => {
    const backend = createMockBackend();
    let failing = true;
    const { user } = renderWithProviders(<App />, {
      ipc: (cmd, args) => {
        if (cmd === 'settings_get' && failing) {
          return Promise.reject({ category: 'game', code: 'internal', message: 'x', fix: null, detail: null, params: {} });
        }
        return backend.handle(cmd, args);
      },
    });
    expect(await screen.findByRole('alert')).toHaveTextContent('Something unexpected went wrong.');
    expect(screen.queryByRole('navigation', { name: 'Instances' })).not.toBeInTheDocument();
    failing = false;
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('navigation', { name: 'Instances' })).toBeInTheDocument();
  });

  it('keeps the error screen up, busy, while a retry of the settings is in flight', async () => {
    const backend = createMockBackend();
    let attempts = 0;
    let release: () => void = () => {};
    const { user } = renderWithProviders(<App />, {
      ipc: (cmd, args) => {
        if (cmd !== 'settings_get') return backend.handle(cmd, args);
        attempts += 1;
        if (attempts === 1) {
          return Promise.reject({ category: 'game', code: 'internal', message: 'x', fix: null, detail: null, params: {} });
        }
        return new Promise((resolve) => {
          release = () => resolve(backend.handle(cmd, args));
        });
      },
    });
    await screen.findByRole('alert');
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(attempts).toBe(2));
    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Loading…' })).toBeDisabled();
    expect(screen.getByRole('banner')).toBeInTheDocument(); // the title bar
    await act(async () => release());
    expect(await screen.findByRole('navigation', { name: 'Instances' })).toBeInTheDocument();
  });

  it('applies the language stored in the settings on load', async () => {
    const backend = createMockBackend();
    renderWithProviders(<App />, {
      ipc: async (cmd, args) => {
        const result = await backend.handle(cmd, args);
        return cmd === 'settings_get' ? { ...(result as object), language: 'es' } : result;
      },
    });
    expect(await screen.findByRole('navigation', { name: 'Instancias' })).toBeInTheDocument();
  });
});
