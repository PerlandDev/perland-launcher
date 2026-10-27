import { emit } from '@tauri-apps/api/event';
import { act, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import { APP_EVENT } from '../../ipc/events';
import { renderApp, sent } from '../../test/backend';
import { MicrosoftSignIn } from './MicrosoftSignIn';

const awaiting = "Microsoft sign-in is waiting on approval from Microsoft. You'll be able to play as soon as it's ready.";

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
    await vi.waitFor(() => expect(onSignedIn).toHaveBeenCalledTimes(1), { timeout: 4000 });
    expect(screen.getByText('Signed in as Alex.')).toBeInTheDocument();
  });

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
  });

  it('awaiting approval shows the line and Try again', async () => {
    const { user, calls } = renderApp(<MicrosoftSignIn variant="primary" />, { beta: true, microsoft: 'awaiting' });
    await user.click(await screen.findByRole('button', { name: 'Sign in with Microsoft' }));
    expect(await screen.findByText(awaiting, {}, { timeout: 4000 })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Sign in with Microsoft' })).not.toBeInTheDocument();
    // Waiting on Microsoft is not the player's fault: it is not an alert.
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    await vi.waitFor(() => expect(sent(calls, 'account_ready').length).toBeGreaterThan(1));
  });

  it('shows a remembered approval wait before any sign-in', async () => {
    const { backend, qc } = renderApp(<MicrosoftSignIn variant="inline" />, { beta: true });
    await screen.findByRole('button', { name: 'Add Microsoft account' });
    backend.state.accounts.msaWaiting = { clientId: 'mock', at: 1 };
    await act(() => qc.invalidateQueries({ queryKey: ['accountReady'] }));
    expect(await screen.findByText(awaiting)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('a minecraft_not_owned failure shows its translated message', async () => {
    const { user } = renderApp(<MicrosoftSignIn variant="primary" />, { beta: true, microsoft: 'notOwned' });
    await user.click(await screen.findByRole('button', { name: 'Sign in with Microsoft' }));
    const alert = await screen.findByRole('alert', {}, { timeout: 4000 });
    expect(alert).toHaveTextContent("This Microsoft account doesn't own Minecraft: Java Edition.");
    expect(alert).toHaveTextContent('Sign in with the account that bought it, or get it at minecraft.net.');
    expect(screen.getByRole('button', { name: 'Sign in with Microsoft' })).toBeInTheDocument();
  });

  it('says it in Spanish', async () => {
    await i18n.changeLanguage('es');
    const { user, calls } = renderApp(<MicrosoftSignIn variant="primary" />, { beta: true, microsoft: 'notConfigured' });
    await user.click(await screen.findByRole('button', { name: 'Iniciar sesión con Microsoft' }));
    expect(sent(calls, 'account_sign_in_microsoft')).toEqual([{ language: 'es' }]);
    expect(await screen.findByRole('alert')).toHaveTextContent('El inicio de sesión con Microsoft no está configurado en esta versión.');
  });
});
