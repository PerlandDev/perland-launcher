import { emit } from '@tauri-apps/api/event';
import { act, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import en from '../../i18n/en.json';
import es from '../../i18n/es.json';
import { APP_EVENT } from '../../ipc/events';
import { qk } from '../../state/queries';
import { renderApp, sent } from '../../test/backend';
import { Profile } from './Profile';

type Tree = { [key: string]: string | Tree };
const keys = (tree: Tree, prefix = ''): string[] =>
  Object.entries(tree).flatMap(([key, value]) => (typeof value === 'string' ? [`${prefix}${key}`] : keys(value, `${prefix}${key}.`)));

/** The Perland account's step in Finish setting up. */
const step = () => screen.getByRole('listitem', { name: /^Perland account/ });
const accounts = () => screen.getByRole('region', { name: 'Accounts' });
/** The signed-in Perland account's row in Accounts. */
const PERLAND_MENU = 'More for your Perland account';
const perlandRow = async () => (await screen.findByRole('button', { name: PERLAND_MENU })).closest('li')!;

/** Your servers of whoever was signed in before: a sign-in or a sign-out must never leave them behind. */
const seedYourServers = (qc: ReturnType<typeof renderApp>['qc']) => qc.setQueryData(qk.yourServers, { servers: [] });

describe('the Perland account on the Profile', () => {
  it('hidden_when_off: shows nothing while the Engine has accounts closed', async () => {
    const { calls } = renderApp(<Profile />, { perland: 'off' });
    await within(accounts()).findByText('Steve');
    await waitFor(() => expect(sent(calls, 'perland_status')).toHaveLength(1));
    await act(async () => {});
    expect(screen.queryByText('Perland account')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Continue with Perland' })).not.toBeInTheDocument();
  });

  it('signed out, is a step of Finish setting up with one key, what it is for, and no row in Accounts', async () => {
    renderApp(<Profile />, { perland: 'signedOut' });
    const key = await screen.findByRole('button', { name: 'Continue with Perland' });
    expect(screen.getByRole('heading', { name: 'Finish setting up' })).toBeInTheDocument();
    expect(within(step()).getAllByRole('button')).toEqual([key]);
    expect(within(step()).queryByRole('textbox')).not.toBeInTheDocument();
    expect(step()).toHaveTextContent('Sign in to create servers and earn medals.');
    expect(step()).toHaveTextContent("You'll choose GitHub, Discord or an email code on the Perland page.");
    // Never lit: the Microsoft sign-in is the page's one lit key.
    expect(key).not.toHaveClass('btn-primary');
    expect(key.querySelector('img[src*="mark"]')).toHaveAttribute('alt', '');
    expect(within(accounts()).queryByText('Perland account')).not.toBeInTheDocument();
  });

  describe('the Perland page (OAuth)', () => {
    const refusal = (code: string) => ({ category: 'auth', code, message: 'x', fix: null, detail: null, params: { host: 'api.example.com' } });

    it('waits on the browser, then signs in: the step goes and the account has its row', async () => {
      const { user, calls, qc } = renderApp(<Profile />, { perland: 'signedOut', microsoft: 'signedIn' });
      seedYourServers(qc);
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      expect(await screen.findByText('Finish signing in with your browser')).toBeInTheDocument();
      expect(screen.getByText("The Perland page is open in your browser. Come back here when you're done.")).toBeInTheDocument();
      expect(sent(calls, 'perland_sign_in')).toEqual([{ language: 'en' }]);
      await user.click(screen.getByRole('button', { name: 'Open the page again' }));
      expect(sent(calls, 'perland_sign_in_reopen')).toHaveLength(1);
      const row = await waitFor(() => perlandRow(), { timeout: 4000 });
      expect(row).toHaveTextContent('Perland account');
      expect(qc.getQueryData(qk.yourServers)).toBeUndefined();
      // Microsoft was already there, so nothing is left to set up: the whole panel goes.
      expect(screen.queryByRole('heading', { name: 'Finish setting up' })).not.toBeInTheDocument();
      expect(screen.getByRole('status')).toHaveTextContent('Signed in to Perland as Steve.');
      // The step is gone: the new row's menu has the keyboard.
      await waitFor(() => expect(screen.getByRole('button', { name: PERLAND_MENU })).toHaveFocus());
    });

    it('Cancel here goes back quietly', async () => {
      const { user, calls } = renderApp(<Profile />, { perland: 'signedOut' });
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      await screen.findByText('Finish signing in with your browser');
      await user.click(screen.getByRole('button', { name: 'Cancel' }));
      expect(await screen.findByRole('button', { name: 'Continue with Perland' })).toHaveFocus();
      expect(sent(calls, 'perland_sign_in_cancel')).toHaveLength(1);
      expect(within(step()).queryByRole('alert')).not.toBeInTheDocument();
      expect(within(step()).queryByRole('status')).not.toBeInTheDocument();
    });

    it('canceled on the page is a quiet note and Try again, not an alert', async () => {
      const { user, calls } = renderApp(<Profile />, { perlandSignIn: 'cancel' });
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      const note = await within(step()).findByRole('status', {}, { timeout: 4000 });
      expect(note).toHaveTextContent('Sign-in was canceled.');
      expect(note).toHaveTextContent('Nothing changed. You can try again whenever you like.');
      expect(within(step()).queryByRole('alert')).not.toBeInTheDocument();
      await user.click(within(step()).getByRole('button', { name: 'Try again' }));
      expect(sent(calls, 'perland_sign_in')).toHaveLength(2);
      expect(within(step()).queryByRole('status')).not.toBeInTheDocument();
    });

    it.each([
      ['timeout', 'Sign-in took too long. Try again.'],
      ['fail', "Couldn't sign you in. Try again."],
      ['expired', 'That sign-in expired before it finished. Try again.'],
    ] as const)('a %s sign-in shows the error in the step, under Try again', async (outcome, message) => {
      const { user } = renderApp(<Profile />, { perlandSignIn: outcome });
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      expect(await within(step()).findByRole('alert', {}, { timeout: 4000 })).toHaveTextContent(message);
      expect(within(step()).getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    });

    it('shows the fix of an error that has one', async () => {
      const { user, backend } = renderApp(<Profile />, { perland: 'signedOut' });
      const handle = backend.handle;
      vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
        cmd === 'perland_sign_in' ? Promise.reject(refusal('sign_in_incompatible')) : handle(cmd, args),
      );
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      const alert = await within(step()).findByRole('alert');
      expect(alert).toHaveTextContent("This Perland signs in a way this launcher doesn't support yet.");
      expect(alert).toHaveTextContent('Update the launcher');
    });

    it('says when the launcher must update before it can sign up', async () => {
      const { user, backend } = renderApp(<Profile />);
      const handle = backend.handle;
      vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
        cmd === 'perland_sign_in' ? Promise.reject(refusal('launcher_update_required')) : handle(cmd, args),
      );
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      const alert = await within(step()).findByRole('alert');
      expect(alert).toHaveTextContent('Update Perland Launcher to sign up.');
      expect(alert).toHaveTextContent('Update Perland Launcher from Settings.');
    });

    it('speaks Spanish', async () => {
      await i18n.changeLanguage('es');
      try {
        const { user, calls } = renderApp(<Profile />, { perlandSignIn: 'cancel' });
        const key = await screen.findByRole('button', { name: 'Continuar con Perland' });
        expect(screen.getByRole('heading', { name: 'Termina de configurar' })).toBeInTheDocument();
        expect(screen.getByText(/Inicia sesión para crear servidores y ganar medallas\./)).toBeInTheDocument();
        expect(screen.getByText(/En la página de Perland eliges GitHub, Discord o un código por correo\./)).toBeInTheDocument();
        await user.click(key);
        expect(await screen.findByText('Cancelaste el inicio de sesión.', {}, { timeout: 4000 })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Intentar de nuevo' })).toBeInTheDocument();
        expect(sent(calls, 'perland_sign_in')).toEqual([{ language: 'es' }]);
      } finally {
        await i18n.changeLanguage('en');
      }
    });

    it('keeps focus on Cancel when Open the page again goes away', async () => {
      const { user, backend } = renderApp(<Profile />, { perland: 'signedOut' });
      const handle = backend.handle;
      // The browser step never ends here: the test moves it to exchanging by hand.
      vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
        cmd === 'perland_sign_in' ? new Promise(() => {}) : handle(cmd, args),
      );
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      const reopen = await screen.findByRole('button', { name: 'Open the page again' });
      expect(reopen).toHaveFocus();
      await act(() => emit(APP_EVENT, { type: 'perland_sign_in', phase: 'exchanging' }));
      expect(await screen.findByText('Signing you in…')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Open the page again' })).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
    });
  });

  it('signed in, is the first row of Accounts, marked as the Perland account, and no step', async () => {
    renderApp(<Profile />, { perland: 'signedIn' });
    const row = await perlandRow();
    expect(within(accounts()).getAllByRole('listitem')[0]).toBe(row);
    expect(row.querySelector('img[src*="mark"]')).toHaveAttribute('alt', '');
    // How it signs in, never its address: a Profile is a page players screenshot and stream.
    expect(row).toHaveTextContent('Perland account · Discord');
    expect(row).not.toHaveTextContent('@');
    expect(screen.queryByRole('button', { name: 'Continue with Perland' })).not.toBeInTheDocument();
  });

  it('a GitHub account shows its login and GitHub as the method', async () => {
    renderApp(<Profile />, { perland: 'github' });
    const row = await perlandRow();
    expect(row).toHaveTextContent('octocat');
    expect(row).toHaveTextContent('Perland account · GitHub');
    expect(row).not.toHaveTextContent('Perland player');
    expect(row).not.toHaveTextContent('Discord');
  });

  it('sign_out_returns_to_signed_out: the row goes, the step is back and has the keyboard', async () => {
    const { user, calls, qc } = renderApp(<Profile />, { perland: 'signedIn' });
    seedYourServers(qc);
    await user.click(await screen.findByRole('button', { name: PERLAND_MENU }));
    await user.click(screen.getByRole('menuitem', { name: 'Sign out' }));
    const key = await screen.findByRole('button', { name: 'Continue with Perland' });
    expect(sent(calls, 'perland_sign_out')).toHaveLength(1);
    expect(qc.getQueryData(qk.yourServers)).toBeUndefined();
    expect(screen.queryByRole('button', { name: PERLAND_MENU })).not.toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Signed out of Perland.');
    await waitFor(() => expect(key).toHaveFocus());
  });

  it('shows a failed sign-out in the row, and stays signed in', async () => {
    const { user, backend } = renderApp(<Profile />, { perland: 'signedIn' });
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
      cmd === 'perland_sign_out'
        ? Promise.reject({ category: 'network', code: 'sign_in_failed', message: 'nope', fix: null, detail: null, params: {} })
        : handle(cmd, args),
    );
    const row = await perlandRow();
    await user.click(screen.getByRole('button', { name: PERLAND_MENU }));
    await user.click(screen.getByRole('menuitem', { name: 'Sign out' }));
    expect(await within(row).findByRole('alert')).toHaveTextContent("Couldn't sign you in. Try again.");
    expect(screen.getByRole('button', { name: PERLAND_MENU })).toBeInTheDocument();
  });

  it('spanish_strings_exist: every perland.* and profile.* key is in es', () => {
    for (const section of ['perland', 'profile']) {
      const english = keys((en as unknown as Tree)[section] as Tree);
      expect(english.length).toBeGreaterThan(0);
      expect(keys((es as unknown as Tree)[section] as Tree).sort()).toEqual(english.sort());
    }
  });
});
