import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import en from '../../i18n/en.json';
import es from '../../i18n/es.json';
import { renderApp, sent } from '../../test/backend';
import { EmailCodeStep } from './EmailCodeStep';

type Tree = { [key: string]: string | Tree };
const keys = (tree: Tree, prefix = ''): string[] =>
  Object.entries(tree).flatMap(([key, value]) => (typeof value === 'string' ? [`${prefix}${key}`] : keys(value, `${prefix}${key}.`)));

type User = ReturnType<typeof renderApp>['user'];

const setup = () => {
  const onVerified = vi.fn();
  const onCancel = vi.fn();
  const rendered = renderApp(<EmailCodeStep onVerified={onVerified} onCancel={onCancel} />, { perland: 'github' });
  return { ...rendered, onVerified, onCancel };
};

async function sendCode(user: User, email: string) {
  await user.type(await screen.findByRole('textbox', { name: 'Email' }), email);
  await user.click(screen.getByRole('button', { name: 'Send code' }));
  await screen.findByText(`We sent a 6-digit code to ${email.trim()}.`);
}

async function tryCode(user: User, code: string) {
  const input = screen.getByRole('textbox', { name: 'Code' });
  await user.clear(input);
  await user.type(input, code);
  await user.click(screen.getByRole('button', { name: 'Verify' }));
}

describe('EmailCodeStep', () => {
  it('says why, sends the trimmed email, then verifies a spaced code', async () => {
    const { user, calls, onVerified } = setup();
    expect(await screen.findByText("Free servers need a verified email. We'll send you a code.")).toBeInTheDocument();
    await sendCode(user, '  a@b.co ');
    expect(sent(calls, 'perland_account_email_start')).toEqual([{ email: 'a@b.co', language: 'en' }]);
    expect(screen.getByRole('button', { name: /Send a new code/ })).toBeDisabled();
    expect(screen.getByRole('textbox', { name: 'Code' })).toHaveFocus();
    await tryCode(user, '123 456');
    await vi.waitFor(() => expect(onVerified).toHaveBeenCalledWith('a@b.co'));
    expect(sent(calls, 'perland_account_email_verify')).toEqual([{ email: 'a@b.co', code: '123 456' }]);
  });

  it('a wrong code says how many tries are left, on the code field', async () => {
    const { user, onVerified } = setup();
    await sendCode(user, 'a@b.co');
    await tryCode(user, '000000');
    expect(await screen.findByRole('alert')).toHaveTextContent("That code isn't right. 4 tries left.");
    expect(screen.getByRole('textbox', { name: 'Code' })).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByRole('textbox', { name: 'Code' })).toHaveFocus();
    expect(onVerified).not.toHaveBeenCalled();
  });

  it('asks for a new code once the tries are used up', async () => {
    const { user } = setup();
    await sendCode(user, 'a@b.co');
    for (let i = 0; i < 5; i++) await tryCode(user, '999999');
    expect(await screen.findByText(/Ask for a new code/)).toBeInTheDocument();
  });

  it('shows a bad email address under the field', async () => {
    const { user } = setup();
    await user.type(await screen.findByRole('textbox', { name: 'Email' }), 'nope');
    await user.click(screen.getByRole('button', { name: 'Send code' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Check the email address.');
    expect(screen.getByRole('textbox', { name: 'Email' })).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByRole('textbox', { name: 'Email' })).toHaveFocus();
  });

  it('shows an email already on another account with its fix', async () => {
    const { user, backend } = setup();
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
      cmd === 'perland_account_email_verify'
        ? Promise.reject({ category: 'auth', code: 'email_in_use', message: 'x', fix: null, detail: null, params: {} })
        : handle(cmd, args),
    );
    await sendCode(user, 'a@b.co');
    await tryCode(user, '123456');
    expect(await screen.findByRole('alert')).toHaveTextContent('That email is already on another Perland account.');
    expect(screen.getByText('Use another email, or sign in with that account.')).toBeInTheDocument();
  });

  it('Use another email goes back to the email field, with the email kept', async () => {
    const { user } = setup();
    await sendCode(user, 'a@b.co');
    await user.click(screen.getByRole('button', { name: 'Use another email' }));
    expect(screen.getByRole('textbox', { name: 'Email' })).toHaveValue('a@b.co');
    expect(screen.getByRole('textbox', { name: 'Email' })).toHaveFocus();
    expect(screen.queryByRole('textbox', { name: 'Code' })).not.toBeInTheDocument();
  });

  it('Cancel leaves', async () => {
    const { user, onCancel } = setup();
    await user.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('speaks Spanish', async () => {
    await i18n.changeLanguage('es');
    try {
      const { user, calls } = setup();
      expect(await screen.findByText('Los servidores gratis necesitan un correo verificado. Te enviaremos un código.')).toBeInTheDocument();
      await user.type(screen.getByRole('textbox', { name: 'Correo electrónico' }), 'a@b.co');
      await user.click(screen.getByRole('button', { name: 'Enviar código' }));
      expect(await screen.findByRole('button', { name: 'Verificar' })).toBeInTheDocument();
      expect(sent(calls, 'perland_account_email_start')).toEqual([{ email: 'a@b.co', language: 'es' }]);
    } finally {
      await i18n.changeLanguage('en');
    }
  });

  it('every freeServer.email key is in es', () => {
    const english = keys(((en as unknown as Tree).freeServer as Tree).email as Tree);
    expect(english.length).toBeGreaterThan(0);
    expect(keys(((es as unknown as Tree).freeServer as Tree).email as Tree).sort()).toEqual(english.sort());
  });
});
