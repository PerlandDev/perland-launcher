import { act, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import App from '../../App';
import { closeSignInPrompt, openSignInPrompt, signInPromptSnapshot } from '../../state/signInPrompt';
import { pushError, toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { SignInPrompt } from './SignInPrompt';

const BODY = 'Perland Launcher needs a Microsoft account that owns Minecraft: Java Edition.';

afterEach(closeSignInPrompt);

describe('SignInPrompt', () => {
  it('shows nothing until opened', async () => {
    renderApp(<SignInPrompt />, { beta: true });
    expect(screen.queryByRole('dialog')).toBeNull();
    act(openSignInPrompt);
    expect(await screen.findByRole('dialog', { name: 'Sign in to play' })).toBeInTheDocument();
    // The prompt's body says why, once: the sign-in under it doesn't repeat it.
    expect(screen.getByText(BODY)).toBeInTheDocument();
    expect(screen.queryByText('You need a Microsoft account that owns Minecraft: Java Edition.')).toBeNull();
  });

  it('signs in, closes, and says to press Play again', async () => {
    const { user, calls } = renderApp(<SignInPrompt />, { beta: true });
    act(openSignInPrompt);
    await user.click(await screen.findByRole('button', { name: 'Sign in with Microsoft' }));
    expect(sent(calls, 'account_sign_in_microsoft')).toHaveLength(1);
    await vi.waitFor(() => expect(signInPromptSnapshot()).toBe(false), { timeout: 4000 });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(toastsSnapshot().map((toast) => toast.notice?.title)).toEqual(['Signed in. Press Play again.']);
  });

  it('Close closes it without signing in', async () => {
    const { user, calls } = renderApp(<SignInPrompt />, { beta: true });
    act(openSignInPrompt);
    await user.click(await screen.findByRole('button', { name: 'Close' }));
    expect(signInPromptSnapshot()).toBe(false);
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(sent(calls, 'account_sign_in_microsoft')).toEqual([]);
  });

  it('is mounted by the app, so a refused Play opens it on any screen', async () => {
    renderApp(<App />, { beta: true });
    await screen.findByRole('button', { name: 'Servers' });
    act(() => pushError({ category: 'auth', code: 'no_account', message: 'm', fix: null, detail: null, params: {} }));
    expect(await screen.findByRole('dialog', { name: 'Sign in to play' })).toBeInTheDocument();
    expect(toastsSnapshot()).toEqual([]);
  });
});
