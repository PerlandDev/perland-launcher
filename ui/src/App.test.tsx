import { emit } from '@tauri-apps/api/event';
import { act, screen } from '@testing-library/react';
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
});
