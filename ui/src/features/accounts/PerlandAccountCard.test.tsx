import { emit } from '@tauri-apps/api/event';
import { act, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import en from '../../i18n/en.json';
import es from '../../i18n/es.json';
import { APP_EVENT } from '../../ipc/events';
import { qk } from '../../state/queries';
import { renderApp, sent } from '../../test/backend';
import { Accounts } from './Accounts';
import { PerlandAccountCard } from './PerlandAccountCard';

type Tree = { [key: string]: string | Tree };
const keys = (tree: Tree, prefix = ''): string[] =>
  Object.entries(tree).flatMap(([key, value]) => (typeof value === 'string' ? [`${prefix}${key}`] : keys(value, `${prefix}${key}.`)));

const card = () => screen.getByRole('region', { name: 'Perland account' });

/** Your servers of whoever was signed in before: a sign-in or a sign-out must never leave them behind. */
const seedYourServers = (qc: ReturnType<typeof renderApp>['qc']) => qc.setQueryData(qk.yourServers, { servers: [] });

describe('PerlandAccountCard', () => {
  it('hidden_when_off: shows nothing while the Engine has accounts closed', async () => {
    const { calls } = renderApp(<Accounts />, { perland: 'off' });
    await screen.findByText('Steve');
    await screen.findByRole('button', { name: 'Add offline account' });
    expect(sent(calls, 'perland_status')).toHaveLength(1);
    expect(screen.queryByText('Perland account')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Continue with Perland' })).not.toBeInTheDocument();
  });

  it('signed out, offers one key and no email field, above the Minecraft accounts', async () => {
    renderApp(<Accounts />, { perland: 'signedOut' });
    const key = await screen.findByRole('button', { name: 'Continue with Perland' });
    expect(within(card()).getAllByRole('button')).toEqual([key]);
    expect(within(card()).queryByRole('textbox')).not.toBeInTheDocument();
    expect(within(card()).getByText('Your servers and free servers go with you.')).toBeInTheDocument();
    const steve = await screen.findByText('Steve');
    expect(key.compareDocumentPosition(steve) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  describe('the Perland page (OAuth)', () => {
    const refusal = (code: string) => ({ category: 'auth', code, message: 'x', fix: null, detail: null, params: { host: 'api.example.com' } });

    it('offers one key, Continue with Perland, and says where the methods are chosen', async () => {
      renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
      const key = await within(await screen.findByRole('region', { name: 'Perland account' })).findByRole('button', { name: 'Continue with Perland' });
      expect(within(card()).getAllByRole('button')).toEqual([key]);
      expect(key).toHaveClass('btn-lg');
      expect(within(card()).getByText("You'll choose GitHub, Discord or an email code on the Perland page.")).toBeInTheDocument();
    });

    it('waits on the browser, then signs in', async () => {
      const { user, calls, qc } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
      seedYourServers(qc);
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      expect(await screen.findByText('Finish signing in with your browser')).toBeInTheDocument();
      expect(screen.getByText("The Perland page is open in your browser. Come back here when you're done.")).toBeInTheDocument();
      expect(sent(calls, 'perland_sign_in')).toEqual([{ language: 'en' }]);
      await user.click(screen.getByRole('button', { name: 'Open the page again' }));
      expect(sent(calls, 'perland_sign_in_reopen')).toHaveLength(1);
      expect(await screen.findByText('Steve', {}, { timeout: 4000 })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Sign out' })).toBeInTheDocument();
      expect(qc.getQueryData(qk.yourServers)).toBeUndefined();
    });

    it('Cancel here goes back quietly', async () => {
      const { user, calls } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      await screen.findByText('Finish signing in with your browser');
      await user.click(screen.getByRole('button', { name: 'Cancel' }));
      expect(await screen.findByRole('button', { name: 'Continue with Perland' })).toBeInTheDocument();
      expect(sent(calls, 'perland_sign_in_cancel')).toHaveLength(1);
      expect(within(card()).queryByRole('alert')).not.toBeInTheDocument();
      expect(within(card()).queryByRole('status')).not.toBeInTheDocument();
    });

    it('canceled on the page is a quiet note and Try again, not an alert', async () => {
      const { user, calls } = renderApp(<PerlandAccountCard />, { perlandSignIn: 'cancel' });
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      const note = await within(card()).findByRole('status', {}, { timeout: 4000 });
      expect(note).toHaveTextContent('Sign-in was canceled.');
      expect(note).toHaveTextContent('Nothing changed. You can try again whenever you like.');
      expect(within(card()).queryByRole('alert')).not.toBeInTheDocument();
      await user.click(within(card()).getByRole('button', { name: 'Try again' }));
      expect(sent(calls, 'perland_sign_in')).toHaveLength(2);
      expect(within(card()).queryByRole('status')).not.toBeInTheDocument();
    });

    it.each([
      ['timeout', 'Sign-in took too long. Try again.'],
      ['fail', "Couldn't sign you in. Try again."],
      ['expired', 'That sign-in expired before it finished. Try again.'],
    ] as const)('a %s sign-in shows the error inline under Try again', async (outcome, message) => {
      const { user } = renderApp(<PerlandAccountCard />, { perlandSignIn: outcome });
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      expect(await within(card()).findByRole('alert', {}, { timeout: 4000 })).toHaveTextContent(message);
      expect(within(card()).getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    });

    it('shows the fix of an error that has one', async () => {
      const { user, backend } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
      const handle = backend.handle;
      vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
        cmd === 'perland_sign_in' ? Promise.reject(refusal('sign_in_incompatible')) : handle(cmd, args),
      );
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      const alert = await within(card()).findByRole('alert');
      expect(alert).toHaveTextContent("This Perland signs in a way this launcher doesn't support yet.");
      expect(alert).toHaveTextContent('Update the launcher');
    });

    it('says when the launcher must update before it can sign up', async () => {
      const { user, backend } = renderApp(<PerlandAccountCard />);
      const handle = backend.handle;
      vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
        cmd === 'perland_sign_in' ? Promise.reject(refusal('launcher_update_required')) : handle(cmd, args),
      );
      await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
      const alert = await within(card()).findByRole('alert');
      expect(alert).toHaveTextContent('Update Perland Launcher to sign up.');
      expect(alert).toHaveTextContent('Update Perland Launcher from Settings.');
    });

    it('speaks Spanish', async () => {
      await i18n.changeLanguage('es');
      try {
        const { user, calls } = renderApp(<PerlandAccountCard />, { perlandSignIn: 'cancel' });
        const key = await screen.findByRole('button', { name: 'Continuar con Perland' });
        expect(screen.getByText('En la página de Perland eliges GitHub, Discord o un código por correo.')).toBeInTheDocument();
        await user.click(key);
        expect(await screen.findByText('Cancelaste el inicio de sesión.', {}, { timeout: 4000 })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Intentar de nuevo' })).toBeInTheDocument();
        expect(sent(calls, 'perland_sign_in')).toEqual([{ language: 'es' }]);
      } finally {
        await i18n.changeLanguage('en');
      }
    });
  });

  it('a GitHub account shows its login and GitHub as the method', async () => {
    renderApp(<PerlandAccountCard />, { perland: 'github' });
    const card = await screen.findByRole('region', { name: 'Perland account' });
    expect(await within(card).findByText('octocat')).toBeInTheDocument();
    expect(within(card).getByText('GitHub')).toBeInTheDocument();
    expect(within(card).queryByText('Perland player')).not.toBeInTheDocument();
    expect(within(card).queryByText('Discord')).not.toBeInTheDocument();
  });

  it('a Discord account shows its email', async () => {
    renderApp(<PerlandAccountCard />, { perland: 'signedIn' });
    expect(await screen.findByText('Steve')).toBeInTheDocument();
    expect(screen.getByText('steve@example.com')).toBeInTheDocument();
  });

  it('sign_out_returns_to_signed_out', async () => {
    const { user, calls, qc } = renderApp(<PerlandAccountCard />, { perland: 'signedIn' });
    seedYourServers(qc);
    expect(await screen.findByText('Steve')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Sign out' }));
    expect(await screen.findByRole('button', { name: 'Continue with Perland' })).toBeInTheDocument();
    expect(sent(calls, 'perland_sign_out')).toHaveLength(1);
    expect(qc.getQueryData(qk.yourServers)).toBeUndefined();
  });

  it('shows a failed sign-out in the signed-in card', async () => {
    const { user, backend } = renderApp(<PerlandAccountCard />, { perland: 'signedIn' });
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
      cmd === 'perland_sign_out'
        ? Promise.reject({ category: 'network', code: 'sign_in_failed', message: 'nope', fix: null, detail: null, params: {} })
        : handle(cmd, args),
    );
    await user.click(await screen.findByRole('button', { name: 'Sign out' }));
    expect(await within(card()).findByRole('alert')).toHaveTextContent("Couldn't sign you in. Try again.");
    expect(screen.getByText('Steve')).toBeInTheDocument();
  });

  it('keeps focus on Cancel when Open the page again goes away', async () => {
    const { user, backend } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
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

  it('spanish_strings_exist: every perland.* key is in es', () => {
    const english = keys((en as unknown as Tree).perland as Tree);
    expect(english.length).toBeGreaterThan(0);
    expect(keys((es as unknown as Tree).perland as Tree).sort()).toEqual(english.sort());
  });
});
