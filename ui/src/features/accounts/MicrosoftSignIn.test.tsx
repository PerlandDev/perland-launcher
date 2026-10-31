import { emit } from '@tauri-apps/api/event';
import { act, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import { APP_EVENT } from '../../ipc/events';
import { renderApp, sent } from '../../test/backend';
import { MicrosoftSignIn } from './MicrosoftSignIn';

const awaiting =
  "Minecraft hasn't approved Perland Launcher yet, so it turned the sign-in down. Your Microsoft account is fine: you'll be able to play as soon as it's approved.";

/** Each row of the sign-in steps, read as a screen reader reads it: the step and its state. */
async function stepRows() {
  const list = await screen.findByRole('list', { name: 'Sign-in steps' }, { timeout: 6000 });
  return within(list)
    .getAllByRole('listitem')
    .map((row) => row.textContent);
}

afterEach(async () => {
  await i18n.changeLanguage('en');
});

describe('MicrosoftSignIn', () => {
  it('says why, and signs in with the launcher language', async () => {
    const onSignedIn = vi.fn();
    const { user, calls } = renderApp(<MicrosoftSignIn variant="primary" onSignedIn={onSignedIn} />, { beta: true });
    expect(await screen.findByText('You need a Microsoft account that owns Minecraft: Java Edition.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Sign in with Microsoft' }));
    expect(sent(calls, 'account_sign_in_microsoft')).toEqual([{ language: 'en' }]);
    await vi.waitFor(() => expect(onSignedIn).toHaveBeenCalledTimes(1), { timeout: 8000 });
    // onSignedIn runs in the same callback that sets the line, which React draws a moment later.
    expect(await screen.findByText('Signed in as Alex.')).toBeInTheDocument();
  }, 20_000);

  it('reads Add Microsoft account inline', async () => {
    renderApp(<MicrosoftSignIn variant="inline" />);
    expect(await screen.findByRole('button', { name: 'Add Microsoft account' })).toBeInTheDocument();
  });

  it('shows the waiting state, and Cancel calls account_sign_in_cancel', async () => {
    const onSignedIn = vi.fn();
    const { user, calls } = renderApp(<MicrosoftSignIn variant="primary" onSignedIn={onSignedIn} />, { beta: true });
    await user.click(await screen.findByRole('button', { name: 'Sign in with Microsoft' }));
    expect(await screen.findByText('Finish signing in in your browser.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Sign in with Microsoft' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(sent(calls, 'account_sign_in_cancel')).toHaveLength(1);
    expect(await screen.findByRole('button', { name: 'Sign in with Microsoft' })).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(onSignedIn).not.toHaveBeenCalled();
  });

  it('says when the sign-in is being finished', async () => {
    const { user } = renderApp(<MicrosoftSignIn variant="primary" />, { beta: true });
    await user.click(await screen.findByRole('button', { name: 'Sign in with Microsoft' }));
    await screen.findByText('Finish signing in in your browser.');
    await act(() => emit(APP_EVENT, { type: 'microsoft_sign_in', phase: 'exchanging' }));
    expect(await screen.findByText('Signing you in…')).toBeInTheDocument();
    // Stop the mock's chain, so its steps can't reach the next test.
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
  });

  it('runs every step and shows each one while it runs', async () => {
    const { user } = renderApp(<MicrosoftSignIn variant="primary" />, { beta: true });
    await user.click(await screen.findByRole('button', { name: 'Sign in with Microsoft' }));
    // In the browser: the Microsoft account step runs, the others wait.
    expect(await stepRows()).toEqual([
      'Microsoft account: in progress',
      'Xbox Live: not reached',
      'Xbox access for Minecraft: not reached',
      'Minecraft sign-in: not reached',
      'Game ownership and profile: not reached',
    ]);
    await act(() => emit(APP_EVENT, { type: 'microsoft_sign_in_step', step: 'xsts' }));
    expect(await stepRows()).toEqual([
      'Microsoft account: done',
      'Xbox Live: done',
      'Xbox access for Minecraft: in progress',
      'Minecraft sign-in: not reached',
      'Game ownership and profile: not reached',
    ]);
    // Canceled, the steps go with the sign-in (and the mock's chain stops, so it can't reach the next test).
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(await screen.findByRole('button', { name: 'Sign in with Microsoft' })).toBeInTheDocument();
    expect(screen.queryByRole('list', { name: 'Sign-in steps' })).not.toBeInTheDocument();
  });

  it('a refusal at Minecraft shows the steps that passed and where it stopped, as info, and offers the full sign-in again', async () => {
    const { user, calls } = renderApp(<MicrosoftSignIn variant="primary" />, { beta: true, microsoft: 'awaiting' });
    await user.click(await screen.findByRole('button', { name: 'Sign in with Microsoft' }));
    expect(await screen.findByText(awaiting, {}, { timeout: 8000 })).toBeInTheDocument();
    expect(await stepRows()).toEqual([
      'Microsoft account: done',
      'Xbox Live: done',
      'Xbox access for Minecraft: done',
      'Minecraft sign-in: waiting on approval',
      'Game ownership and profile: not reached',
    ]);
    // Waiting on approval is not the player's fault: it is not an alert.
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    // The key is the same sign-in: pressing it runs the whole chain again.
    await user.click(screen.getByRole('button', { name: 'Sign in with Microsoft' }));
    expect(sent(calls, 'account_sign_in_microsoft')).toHaveLength(2);
  }, 20_000);

  it('says nothing about approval before a sign-in', async () => {
    renderApp(<MicrosoftSignIn variant="inline" />, { beta: true, microsoft: 'awaiting' });
    expect(await screen.findByRole('button', { name: 'Add Microsoft account' })).toBeInTheDocument();
    expect(screen.queryByText(awaiting)).not.toBeInTheDocument();
    expect(screen.queryByRole('list', { name: 'Sign-in steps' })).not.toBeInTheDocument();
  });

  it('a minecraft_not_owned failure shows its translated message', async () => {
    const { user } = renderApp(<MicrosoftSignIn variant="primary" />, { beta: true, microsoft: 'notOwned' });
    await user.click(await screen.findByRole('button', { name: 'Sign in with Microsoft' }));
    const alert = await screen.findByRole('alert', {}, { timeout: 8000 });
    expect(alert).toHaveTextContent("This Microsoft account doesn't own Minecraft: Java Edition.");
    expect(alert).toHaveTextContent('Sign in with the account that bought it, or get it at minecraft.net.');
    expect(screen.getByRole('button', { name: 'Sign in with Microsoft' })).toBeInTheDocument();
    expect((await stepRows()).at(-1)).toBe('Game ownership and profile: stopped here');
  }, 20_000);

  it('says it in Spanish', async () => {
    await i18n.changeLanguage('es');
    const { user, calls } = renderApp(<MicrosoftSignIn variant="primary" />, { beta: true, microsoft: 'notConfigured' });
    await user.click(await screen.findByRole('button', { name: 'Iniciar sesión con Microsoft' }));
    expect(sent(calls, 'account_sign_in_microsoft')).toEqual([{ language: 'es' }]);
    expect(await screen.findByRole('alert')).toHaveTextContent('El inicio de sesión con Microsoft no está configurado en esta versión.');
  });
});
