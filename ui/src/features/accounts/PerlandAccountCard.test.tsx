import { emit } from '@tauri-apps/api/event';
import { act, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import en from '../../i18n/en.json';
import es from '../../i18n/es.json';
import { APP_EVENT } from '../../ipc/events';
import { MOCK_EMAIL_CODE } from '../../ipc/mockPerland';
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

async function startEmail(user: ReturnType<typeof renderApp>['user'], email = 'alex@example.com') {
  await user.click(await screen.findByRole('button', { name: 'Continue with email' }));
  await user.type(screen.getByRole('textbox', { name: 'Email' }), email);
  await user.click(screen.getByRole('button', { name: 'Send code' }));
  await screen.findByText(`We sent a 6-digit code to ${email.trim()}.`);
}

async function tryCode(user: ReturnType<typeof renderApp>['user'], code: string) {
  const input = screen.getByRole('textbox', { name: 'Code' });
  await user.clear(input);
  await user.type(input, code);
  await user.click(screen.getByRole('button', { name: 'Sign in' }));
}

describe('PerlandAccountCard', () => {
  it('hidden_when_off: shows nothing while the Engine has accounts closed', async () => {
    const { calls } = renderApp(<Accounts />, { perland: 'off' });
    await screen.findByText('Steve');
    await screen.findByRole('button', { name: 'Add offline account' });
    expect(sent(calls, 'perland_status')).toHaveLength(1);
    expect(screen.queryByText('Perland account')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Continue with Discord' })).not.toBeInTheDocument();
  });

  it('signed_out_shows_both_methods, above the Minecraft accounts', async () => {
    renderApp(<Accounts />, { perland: 'signedOut' });
    const discord = await screen.findByRole('button', { name: 'Continue with Discord' });
    expect(within(card()).getByRole('button', { name: 'Continue with email' })).toBeInTheDocument();
    expect(within(card()).getByText('Your servers and free servers go with you.')).toBeInTheDocument();
    // The card comes before the Minecraft list.
    const steve = await screen.findByText('Steve');
    expect(discord.compareDocumentPosition(steve) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('email_only_shows_email', async () => {
    renderApp(<PerlandAccountCard />, { perland: 'emailOnly' });
    await screen.findByRole('button', { name: 'Continue with email' });
    expect(screen.queryByRole('button', { name: 'Continue with Discord' })).not.toBeInTheDocument();
  });

  it('discord_shows_waiting_then_signed_in', async () => {
    const { user, calls, qc } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    seedYourServers(qc);
    await user.click(await screen.findByRole('button', { name: 'Continue with Discord' }));
    expect(await screen.findByText('Finish signing in with your browser')).toBeInTheDocument();
    expect(sent(calls, 'perland_sign_in_discord')).toEqual([{ language: 'en' }]);

    await user.click(screen.getByRole('button', { name: 'Open the page again' }));
    expect(sent(calls, 'perland_sign_in_reopen')).toHaveLength(1);

    expect(await screen.findByText('Steve', {}, { timeout: 4000 })).toBeInTheDocument();
    expect(screen.getByText('steve@example.com')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Sign out' })).toBeInTheDocument();
    expect(qc.getQueryData(qk.yourServers)).toBeUndefined();
  });

  it('Continue with Perland shows only when the Engine offers github', async () => {
    // The mock offers GitHub only while a panel is configured.
    renderApp(<PerlandAccountCard />, { perland: 'signedOut', panel: false });
    await screen.findByRole('button', { name: 'Continue with Discord' });
    expect(screen.queryByRole('button', { name: 'Continue with Perland' })).not.toBeInTheDocument();
    expect(within(card()).getAllByRole('button').map((b) => b.textContent)).toEqual(['Continue with Discord', 'Continue with email']);
  });

  it('puts Continue with Perland first, as the large key, and every key wears a hidden icon', async () => {
    renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    const perland = await screen.findByRole('button', { name: 'Continue with Perland' });
    const buttons = within(card()).getAllByRole('button');
    expect(buttons.map((b) => b.textContent)).toEqual(['Continue with Perland', 'Continue with Discord', 'Continue with email']);
    // The lead key is large but raised, never lit: Accounts keeps one amber key.
    expect(perland).toHaveClass('btn-lg');
    expect(perland).not.toHaveClass('btn-primary');
    // Every icon is decorative: the mark has empty alt text, the glyphs are hidden, and each key is named by its label alone.
    const [, discord, email] = buttons;
    expect(perland.querySelector('img')).toHaveAttribute('alt', '');
    expect(discord?.querySelector('svg[data-icon="discord"]')).toHaveAttribute('aria-hidden', 'true');
    expect(email?.querySelector('svg[data-icon="mail"]')).toHaveAttribute('aria-hidden', 'true');
  });

  it('names the Perland key in Spanish', async () => {
    await i18n.changeLanguage('es');
    renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    expect(await screen.findByRole('button', { name: 'Continuar con Perland' })).toBeInTheDocument();
  });

  it('Continue with Perland calls perland_sign_in_github', async () => {
    const { user, calls, qc } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    seedYourServers(qc);
    await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
    expect(await screen.findByText('Finish signing in with your browser')).toBeInTheDocument();
    expect(sent(calls, 'perland_sign_in_github')).toEqual([{ language: 'en' }]);
    expect(sent(calls, 'perland_sign_in_discord')).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Open the page again' }));
    expect(sent(calls, 'perland_sign_in_reopen')).toHaveLength(1);
    expect(await screen.findByText('octocat', {}, { timeout: 4000 })).toBeInTheDocument();
    expect(screen.getByText('GitHub')).toBeInTheDocument();
    expect(qc.getQueryData(qk.yourServers)).toBeUndefined();
  });

  it('a GitHub account shows its login and GitHub as the method', async () => {
    renderApp(<PerlandAccountCard />, { perland: 'github' });
    const card = await screen.findByRole('region', { name: 'Perland account' });
    expect(await within(card).findByText('octocat')).toBeInTheDocument();
    expect(within(card).getByText('GitHub')).toBeInTheDocument();
    expect(within(card).queryByText('Perland player')).not.toBeInTheDocument();
    expect(within(card).queryByText('Discord')).not.toBeInTheDocument();
  });

  it('the Perland sign-in can be canceled', async () => {
    const { user, calls } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    await user.click(await screen.findByRole('button', { name: 'Continue with Perland' }));
    await screen.findByText('Finish signing in with your browser');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(await screen.findByRole('button', { name: 'Continue with Perland' })).toBeInTheDocument();
    expect(sent(calls, 'perland_sign_in_cancel')).toHaveLength(1);
  });

  it('cancel_returns_to_signed_out', async () => {
    const { user, calls } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    await user.click(await screen.findByRole('button', { name: 'Continue with Discord' }));
    await screen.findByText('Finish signing in with your browser');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(await screen.findByRole('button', { name: 'Continue with Discord' })).toBeInTheDocument();
    expect(sent(calls, 'perland_sign_in_cancel')).toHaveLength(1);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('shows a Discord sign-in failure inline, not as a toast', async () => {
    const { user, backend } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
      cmd === 'perland_sign_in_discord'
        ? Promise.reject({ category: 'network', code: 'sign_in_timed_out', message: 'late', fix: null, detail: null, params: {} })
        : handle(cmd, args),
    );
    await user.click(await screen.findByRole('button', { name: 'Continue with Discord' }));
    expect(await within(card()).findByRole('alert')).toHaveTextContent('Sign-in took too long. Try again.');
    expect(within(card()).getByRole('button', { name: 'Continue with Discord' })).toBeInTheDocument();
  });

  it('says a timed-out sign-in in Spanish', async () => {
    const { default: i18n } = await import('../../i18n');
    await i18n.changeLanguage('es');
    const { user, backend } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
      cmd === 'perland_sign_in_discord'
        ? Promise.reject({ category: 'network', code: 'sign_in_timed_out', message: 'late', fix: null, detail: null, params: {} })
        : handle(cmd, args),
    );
    await user.click(await screen.findByRole('button', { name: 'Continuar con Discord' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('El inicio de sesión tardó demasiado. Inténtalo de nuevo.');
  });

  it('email_wrong_then_right_code, with Send a new code waiting', async () => {
    const { user, calls, qc } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    seedYourServers(qc);
    await startEmail(user, '  alex@example.com ');
    expect(sent(calls, 'perland_email_start')).toEqual([{ email: 'alex@example.com', language: 'en' }]);
    expect(screen.getByRole('button', { name: /Send a new code/ })).toBeDisabled();

    await tryCode(user, '000000');
    expect(await screen.findByText("That code isn't right. 4 tries left.")).toBeInTheDocument();
    // Announced, and the player is back on the code to fix it.
    expect(screen.getByRole('alert')).toHaveTextContent("That code isn't right. 4 tries left.");
    expect(screen.getByRole('textbox', { name: 'Code' })).toHaveFocus();

    // Spaces are fine: the launcher strips them.
    const spaced = `${MOCK_EMAIL_CODE.slice(0, 3)} ${MOCK_EMAIL_CODE.slice(3)}`;
    await tryCode(user, ` ${spaced} `);
    expect(sent(calls, 'perland_email_verify').at(-1)).toEqual({ email: 'alex@example.com', code: spaced });
    expect(await screen.findAllByText('alex@example.com')).not.toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Sign out' })).toBeInTheDocument();
    expect(qc.getQueryData(qk.yourServers)).toBeUndefined();
  });

  it('attempts_exhausted_asks_for_new_code', async () => {
    const { user } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    await startEmail(user);
    for (let i = 0; i < 5; i++) await tryCode(user, '999999');
    expect(await screen.findByText(/Ask for a new code/)).toBeInTheDocument();
  });

  it('asks in Spanish when the launcher is in Spanish', async () => {
    await i18n.changeLanguage('es');
    const { user, calls } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    await user.click(await screen.findByRole('button', { name: 'Continuar con Discord' }));
    expect(await screen.findByText('Termina de iniciar sesión en tu navegador')).toBeInTheDocument();
    expect(sent(calls, 'perland_sign_in_discord')).toEqual([{ language: 'es' }]);
    await user.click(screen.getByRole('button', { name: 'Cancelar' }));
  });

  it('shows a bad email address under the field', async () => {
    const { user } = renderApp(<PerlandAccountCard />, { perland: 'signedOut' });
    await user.click(await screen.findByRole('button', { name: 'Continue with email' }));
    await user.type(screen.getByRole('textbox', { name: 'Email' }), 'nope');
    await user.click(screen.getByRole('button', { name: 'Send code' }));
    expect(await screen.findByText('Check the email address.')).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Email' })).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByRole('alert')).toHaveTextContent('Check the email address.');
    expect(screen.getByRole('textbox', { name: 'Email' })).toHaveFocus();
  });

  it('sign_out_returns_to_signed_out', async () => {
    const { user, calls, qc } = renderApp(<PerlandAccountCard />, { perland: 'signedIn' });
    seedYourServers(qc);
    expect(await screen.findByText('Steve')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Sign out' }));
    expect(await screen.findByRole('button', { name: 'Continue with Discord' })).toBeInTheDocument();
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
      cmd === 'perland_sign_in_discord' ? new Promise(() => {}) : handle(cmd, args),
    );
    await user.click(await screen.findByRole('button', { name: 'Continue with Discord' }));
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
