import { emit } from '@tauri-apps/api/event';
import { act, screen, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { CLOSE_REQUESTED } from '../../ipc/events';
import { toastsSnapshot } from '../../state/toasts';
import { renderWithProviders } from '../../test/render';
import { CloseGuard } from './CloseGuard';

const failure = { category: 'game', code: 'internal', message: 'boom', fix: null, detail: null, params: {} };
const DIALOG = { name: 'A game is still running' };

async function askToClose() {
  await act(() => emit(CLOSE_REQUESTED));
  return screen.findByRole('dialog', DIALOG);
}

describe('CloseGuard', () => {
  it('stays hidden until the window asks to close', async () => {
    renderWithProviders(<CloseGuard />);
    expect(screen.queryByRole('dialog', DIALOG)).not.toBeInTheDocument();
    await askToClose();
  });

  it('minimizes instead and closes the dialog', async () => {
    const seen: string[] = [];
    const { user } = renderWithProviders(<CloseGuard />, {
      ipc: (cmd) => {
        seen.push(cmd);
        return null;
      },
    });
    await askToClose();
    await user.click(screen.getByRole('button', { name: 'Minimize instead' }));
    expect(seen).toContain('plugin:window|minimize');
    await waitFor(() => expect(screen.queryByRole('dialog', DIALOG)).not.toBeInTheDocument());
  });

  it('shows a failure to minimize', async () => {
    const { user } = renderWithProviders(<CloseGuard />, {
      ipc: (cmd) => (cmd === 'plugin:window|minimize' ? Promise.reject(failure) : null),
    });
    await askToClose();
    await user.click(screen.getByRole('button', { name: 'Minimize instead' }));
    await waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
  });

  it('sends one quit however many times the button is pressed', async () => {
    const quits: unknown[] = [];
    const { user } = renderWithProviders(<CloseGuard />, {
      ipc: (cmd, args) => {
        if (cmd !== 'quit') return null;
        quits.push(args);
        return new Promise(() => {}); // the app is closing; it never answers
      },
    });
    await askToClose();
    const quit = screen.getByRole('button', { name: 'Quit and stop games' });
    await user.dblClick(quit);
    await user.click(quit);
    expect(quits).toEqual([{ stopGames: true }]);
  });

  it('shows a failed quit and lets the player try again', async () => {
    let quits = 0;
    const { user } = renderWithProviders(<CloseGuard />, {
      ipc: (cmd) => {
        if (cmd !== 'quit') return null;
        quits += 1;
        return Promise.reject(failure);
      },
    });
    await askToClose();
    const quit = screen.getByRole('button', { name: 'Quit and stop games' });
    await user.click(quit);
    await waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
    await user.click(quit);
    await waitFor(() => expect(quits).toBe(2));
  });
});
