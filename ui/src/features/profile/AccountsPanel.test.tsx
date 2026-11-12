import { act, screen, waitFor, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import { MOCK_MICROSOFT_ACCOUNT, MOCK_SKIN_FACE } from '../../ipc/mock';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { Profile } from './Profile';

type User = ReturnType<typeof renderApp>['user'];

const failure = { category: 'storage', code: 'disk_io', message: 'nope', fix: null, detail: null, params: {} };
const diskIo = "Couldn't read or write a launcher file.";
const rule = 'Player names must be 3–16 letters, numbers or underscores.';

/** Intercept one backend command: `hold` parks it until `release`, `fail` rejects it. */
function intercept(backend: ReturnType<typeof renderApp>['backend'], command: string) {
  const handle = backend.handle;
  const control = { mode: 'pass' as 'pass' | 'hold' | 'fail', release: () => {} };
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => {
    if (cmd !== command || control.mode === 'pass') return handle(cmd, args);
    if (control.mode === 'fail') return Promise.reject(failure);
    return new Promise((resolve) => (control.release = () => resolve(handle(cmd, args))));
  });
  return control;
}

const panel = () => screen.getByRole('region', { name: 'Accounts' });
/** An account's row in the panel, once it is there. */
const row = async (name: string) => (await within(panel()).findByText(name)).closest('li')!;
const hasRow = (name: string) => within(panel()).queryByText(name) !== null;
const menuKey = (name: string) => screen.getByRole('button', { name: `More for ${name}` });
/** Open an account's menu and pick one of its items. */
async function pick(user: User, name: string, item: string) {
  await user.click(await screen.findByRole('button', { name: `More for ${name}` }));
  await user.click(screen.getByRole('menuitem', { name: item }));
}
/** Remove an account through its menu and the dialog that asks first. */
async function removeAccount(user: User, name: string) {
  await pick(user, name, 'Remove');
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
}
/** Open Add an account and hand back its dialog. */
async function openAdd(user: User) {
  await user.click(await screen.findByRole('button', { name: 'Add an account' }));
  return screen.getByRole('dialog', { name: 'Add an account' });
}

it('switches, adds and removes offline accounts', async () => {
  const { user, calls } = renderApp(<Profile />);
  const alex = await row('Alex_Builds');
  expect(within(await row('Steve')).getByText('In use')).toBeInTheDocument();
  await user.click(within(alex).getByRole('button', { name: 'Use' }));
  expect(sent(calls, 'account_set_active')).toHaveLength(1);

  const dialog = await openAdd(user);
  const input = within(dialog).getByRole('textbox', { name: 'Player name' });
  await user.type(input, 'no spaces!');
  await user.click(within(dialog).getByRole('button', { name: 'Add offline account' }));
  expect(within(dialog).getByText(rule)).toBeInTheDocument();
  expect(sent(calls, 'account_add_offline')).toEqual([]);
  await user.clear(input);
  await user.type(input, 'Notch');
  await user.click(within(dialog).getByRole('button', { name: 'Add offline account' }));
  expect(sent(calls, 'account_add_offline')).toEqual([{ name: 'Notch' }]);
  // Added: the dialog closes and the account has its row.
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(await row('Notch')).toBeInTheDocument();

  await removeAccount(user, 'Alex_Builds');
  expect(sent(calls, 'account_remove')).toHaveLength(1);
});

it('offers Edit skin for each account, wearing what is made on that account', async () => {
  const { user, backend } = renderApp(<Profile />);
  await pick(user, 'Alex_Builds', 'Edit skin');
  const alex = backend.state.accounts.accounts.find((a) => a.username === 'Alex_Builds')!;
  expect(routeSnapshot()).toEqual({ name: 'skins', account: alex.uuid });
});

it('beta shows Add Microsoft account and never Add offline account', async () => {
  const { user } = renderApp(<Profile />, { beta: true, offlineLocked: true });
  const dialog = await openAdd(user);
  expect(within(dialog).getByRole('button', { name: 'Add Microsoft account' })).toBeInTheDocument();
  expect(within(dialog).queryByText('Add offline account')).not.toBeInTheDocument();
  expect(within(dialog).queryByRole('textbox', { name: 'Player name' })).not.toBeInTheDocument();
  // The fixture's offline accounts are hidden, as the real app hides them.
  expect(hasRow('Steve')).toBe(false);
  expect(within(panel()).getByText('No accounts yet.')).toBeInTheDocument();
});

it('says under each name which kind of account it is (#157)', async () => {
  renderApp(<Profile />, { microsoft: 'signedIn' });
  const alex = await row('Alex');
  const steve = await row('Steve');
  expect(within(alex).getByText('Microsoft account')).toBeInTheDocument();
  expect(within(alex).queryByText('Offline account')).not.toBeInTheDocument();
  expect(within(steve).getByText('Offline account')).toBeInTheDocument();
  expect(within(steve).queryByText('Microsoft account')).not.toBeInTheDocument();
});

it('says the kind in Spanish too', async () => {
  renderApp(<Profile />, { microsoft: 'signedIn' });
  await act(() => i18n.changeLanguage('es'));
  try {
    const cuentas = screen.getByRole('region', { name: 'Cuentas' });
    expect(within((await within(cuentas).findByText('Alex')).closest('li')!).getByText('Cuenta de Microsoft')).toBeInTheDocument();
    expect(within(within(cuentas).getByText('Steve').closest('li')!).getByText('Cuenta sin conexión')).toBeInTheDocument();
  } finally {
    await act(() => i18n.changeLanguage('en'));
  }
});

it('lists a Microsoft account with Remove', async () => {
  const { user, calls } = renderApp(<Profile />, { beta: true, offlineLocked: true, microsoft: 'signedIn' });
  expect(within(await row('Alex')).getByText('In use')).toBeInTheDocument();
  await pick(user, 'Alex', 'Remove');
  const dialog = screen.getByRole('dialog');
  expect(dialog).toHaveTextContent('You can sign in again any time. Your worlds stay where they are.');
  await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
  expect(sent(calls, 'account_remove')).toHaveLength(1);
  await waitFor(() => expect(hasRow('Alex')).toBe(false));
});

it("shows a Microsoft account's own skin face and the generated face for offline accounts", async () => {
  const { calls } = renderApp(<Profile />, { microsoft: 'signedIn' });
  const alex = await row('Alex');
  await waitFor(() => expect(alex.querySelector('img')).toHaveAttribute('src', MOCK_SKIN_FACE));
  const steve = await row('Steve');
  expect(steve.querySelector('img')).toBeNull();
  expect(steve.querySelector('svg rect')).not.toBeNull();
  expect(sent(calls, 'account_face')).toEqual([{ uuid: MOCK_MICROSOFT_ACCOUNT.uuid }]);
});

it('keeps the generated face when the skin face cannot be shown', async () => {
  const { backend } = renderApp(<Profile />, { microsoft: 'signedIn' });
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
    cmd === 'account_face' ? Promise.resolve('data:image/png;base64,broken') : handle(cmd, args),
  );
  const alex = await row('Alex');
  const img = await waitFor(() => {
    const found = alex.querySelector('img');
    expect(found).not.toBeNull();
    return found!;
  });
  act(() => {
    img.dispatchEvent(new Event('error'));
  });
  expect(alex.querySelector('img')).toBeNull();
  expect(alex.querySelector('svg rect')).not.toBeNull();
});

it('adds a Microsoft account from the dialog and says who is in use', async () => {
  const { user, calls } = renderApp(<Profile />, { beta: true, offlineLocked: true });
  const dialog = await openAdd(user);
  await user.click(within(dialog).getByRole('button', { name: 'Add Microsoft account' }));
  expect(await within(panel()).findByText('Alex', {}, { timeout: 4000 })).toBeInTheDocument();
  expect(sent(calls, 'account_sign_in_microsoft')).toEqual([{ language: 'en' }]);
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(screen.getByRole('status')).toHaveTextContent('Alex is now in use.');
});

it('offers both kinds in a dev build', async () => {
  const { user } = renderApp(<Profile />);
  const dialog = await openAdd(user);
  expect(within(dialog).getByRole('button', { name: 'Add Microsoft account' })).toBeInTheDocument();
  expect(within(dialog).getByRole('button', { name: 'Add offline account' })).toBeInTheDocument();
});

it('opens Add an account on arrival when the player came to add one', async () => {
  renderApp(<Profile add />);
  expect(await screen.findByRole('dialog', { name: 'Add an account' })).toBeInTheDocument();
});

it('does not say the name is wrong until the player has typed it and tried to add it', async () => {
  const { user } = renderApp(<Profile />);
  const dialog = await openAdd(user);
  const input = within(dialog).getByRole('textbox', { name: 'Player name' });
  expect(screen.queryByText(rule)).not.toBeInTheDocument();
  await user.type(input, 'ab');
  expect(screen.queryByText(rule)).not.toBeInTheDocument();
  expect(input).not.toHaveAttribute('aria-invalid');

  await user.click(within(dialog).getByRole('button', { name: 'Add offline account' }));
  const message = screen.getByText(rule);
  // Focus lands on the field, so its description is read and the player can fix it.
  expect(input).toHaveFocus();
  expect(input).toHaveAttribute('aria-invalid', 'true');
  expect(input.getAttribute('aria-describedby')).toBe(message.id);

  // The message follows the name: it goes away as soon as the name is fine.
  await user.type(input, 'c');
  expect(screen.queryByText(rule)).not.toBeInTheDocument();
});

it('shows the name rule when the player leaves a typed name, but not an empty field', async () => {
  const { user } = renderApp(<Profile />);
  const dialog = await openAdd(user);
  const input = within(dialog).getByRole('textbox', { name: 'Player name' });
  await user.click(input);
  await user.tab();
  expect(screen.queryByText(rule)).not.toBeInTheDocument();
  await user.click(input);
  await user.type(input, 'a b');
  await user.tab();
  expect(screen.getByText(rule)).toBeInTheDocument();
});

it('sends one switch for a double click, says who is in use now, and hands the keyboard to the row', async () => {
  const { user, calls, backend } = renderApp(<Profile />);
  const use = intercept(backend, 'account_set_active');
  use.mode = 'hold';
  const alex = await row('Alex_Builds');
  await user.dblClick(within(alex).getByRole('button', { name: 'Use' }));
  expect(sent(calls, 'account_set_active')).toHaveLength(1);
  await act(async () => use.release());
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Alex_Builds is now in use.'));
  expect(within(await row('Alex_Builds')).getByText('In use')).toBeInTheDocument();
  // The Use key is gone: the row's menu has the keyboard, not the page.
  await waitFor(() => expect(menuKey('Alex_Builds')).toHaveFocus());
});

it('asks for the list again after a failed switch so it never goes stale', async () => {
  const { user, calls, backend } = renderApp(<Profile />);
  const alex = await row('Alex_Builds');
  intercept(backend, 'account_set_active').mode = 'fail';
  const before = sent(calls, 'accounts_list').length;
  await user.click(within(alex).getByRole('button', { name: 'Use' }));
  await waitFor(() => expect(sent(calls, 'accounts_list').length).toBe(before + 1));
  expect(screen.getByRole('status')).toBeEmptyDOMElement();
});

it('sends one add for a double click, and shows a failed add under the form with the name kept', async () => {
  const { user, calls, backend } = renderApp(<Profile />);
  const add = intercept(backend, 'account_add_offline');
  let dialog = await openAdd(user);
  await user.type(within(dialog).getByRole('textbox', { name: 'Player name' }), 'Notch');
  add.mode = 'hold';
  await user.dblClick(within(dialog).getByRole('button', { name: 'Add offline account' }));
  expect(sent(calls, 'account_add_offline')).toHaveLength(1);
  await act(async () => add.release());
  expect(await row('Notch')).toBeInTheDocument();
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());

  add.mode = 'fail';
  dialog = await openAdd(user);
  const input = within(dialog).getByRole('textbox', { name: 'Player name' });
  expect(input).toHaveValue('');
  await user.type(input, 'Herobrine');
  await user.click(within(dialog).getByRole('button', { name: 'Add offline account' }));
  expect(await within(dialog).findByRole('alert')).toHaveTextContent(diskIo);
  expect(input).toHaveValue('Herobrine');
});

it('keeps the add button focused while an add is in flight', async () => {
  const { user, backend } = renderApp(<Profile />);
  intercept(backend, 'account_add_offline').mode = 'hold';
  const dialog = await openAdd(user);
  await user.type(within(dialog).getByRole('textbox', { name: 'Player name' }), 'Notch');
  const button = within(dialog).getByRole('button', { name: 'Add offline account' });
  button.focus();
  await user.keyboard('{Enter}');
  expect(button).toHaveFocus();
  expect(button).not.toBeDisabled();
  expect(button).toHaveAttribute('aria-disabled', 'true');
});

it('says what removing loses and that it can be added again, and sends one remove for a double click', async () => {
  const { user, calls, backend } = renderApp(<Profile />);
  const remove = intercept(backend, 'account_remove');
  remove.mode = 'hold';
  await pick(user, 'Alex_Builds', 'Remove');
  const dialog = screen.getByRole('dialog', { name: 'Remove Alex_Builds?' });
  expect(dialog).toHaveTextContent('You can add it again any time. Your worlds stay where they are.');
  await user.dblClick(within(dialog).getByRole('button', { name: 'Remove' }));
  expect(sent(calls, 'account_remove')).toHaveLength(1);
  await act(async () => remove.release());
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(hasRow('Alex_Builds')).toBe(false);
});

it('keeps the dialog open and shows why a remove failed inside it', async () => {
  const { user, calls, backend } = renderApp(<Profile />);
  await row('Alex_Builds');
  intercept(backend, 'account_remove').mode = 'fail';
  const before = sent(calls, 'accounts_list').length;
  await pick(user, 'Alex_Builds', 'Remove');
  const dialog = screen.getByRole('dialog');
  await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
  expect(await within(dialog).findByRole('alert')).toHaveTextContent(diskIo);
  await waitFor(() => expect(sent(calls, 'accounts_list').length).toBe(before + 1));
  // Cancelling and reopening starts clean.
  await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
  await pick(user, 'Alex_Builds', 'Remove');
  expect(within(screen.getByRole('dialog')).queryByRole('alert')).not.toBeInTheDocument();
});

it('offers Try again when the list fails to load', async () => {
  const { user, backend, qc } = renderApp(<Profile />);
  await row('Steve');
  const list = intercept(backend, 'accounts_list');
  list.mode = 'fail';
  await act(async () => qc.resetQueries({ queryKey: ['accounts'] }));
  expect(await within(panel()).findByRole('alert')).toHaveTextContent(diskIo);
  expect(screen.queryByText('No accounts yet.')).not.toBeInTheDocument();
  // Whether a Microsoft account is missing isn't known without the list, so it isn't asked for.
  expect(screen.queryByRole('button', { name: 'Sign in with Microsoft' })).not.toBeInTheDocument();
  list.mode = 'pass';
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  expect(await row('Steve')).toBeInTheDocument();
  expect(within(panel()).queryByRole('alert')).not.toBeInTheDocument();
});

it('says nothing about adding it back while it is still unknown whether offline accounts are allowed', async () => {
  const { user, backend, qc } = renderApp(<Profile />);
  await row('Steve');
  const info = intercept(backend, 'app_info');
  info.mode = 'hold';
  act(() => {
    void qc.resetQueries({ queryKey: ['appInfo'] }); // parked, so the answer never arrives
  });
  await pick(user, 'Alex_Builds', 'Remove');
  const dialog = screen.getByRole('dialog');
  expect(dialog).toHaveTextContent('Your worlds stay where they are.');
  expect(dialog).not.toHaveTextContent(/add it again|Microsoft/);
});

it('moves focus to the next account after a remove, and to Add an account when none are left', async () => {
  const { user } = renderApp(<Profile />);
  await removeAccount(user, 'Steve');
  await waitFor(() => expect(menuKey('Alex_Builds')).toHaveFocus());
  await removeAccount(user, 'Alex_Builds');
  await waitFor(() => expect(screen.getByRole('button', { name: 'Add an account' })).toHaveFocus());
});

it('moves focus to the previous account when the last one is removed', async () => {
  const { user } = renderApp(<Profile />);
  await removeAccount(user, 'Alex_Builds');
  await waitFor(() => expect(menuKey('Steve')).toHaveFocus());
});

it('announces the new active account after an add and after removing the active one', async () => {
  const { user } = renderApp(<Profile />);
  const dialog = await openAdd(user);
  await user.type(within(dialog).getByRole('textbox', { name: 'Player name' }), 'Notch');
  await user.click(within(dialog).getByRole('button', { name: 'Add offline account' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Notch is now in use.'));

  // Removing an account that is not in use changes nothing about who is.
  await removeAccount(user, 'Steve');
  expect(screen.getByRole('status')).toHaveTextContent('Notch is now in use.');

  // Removing the one in use hands over to the first one left.
  await removeAccount(user, 'Notch');
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Alex_Builds is now in use.'));
});

it('clears the announcement when removing the last account leaves nobody in use', async () => {
  const { user } = renderApp(<Profile />);
  await user.click(within(await row('Alex_Builds')).getByRole('button', { name: 'Use' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Alex_Builds is now in use.'));
  await removeAccount(user, 'Steve');
  await removeAccount(user, 'Alex_Builds');
  expect(screen.getByRole('status')).toBeEmptyDOMElement();
});

it('names who will be used instead when the account in use is being removed', async () => {
  const { user } = renderApp(<Profile />);
  // Steve is in use; Alex_Builds is the first one left.
  await pick(user, 'Steve', 'Remove');
  let dialog = screen.getByRole('dialog');
  expect(dialog).toHaveTextContent('Alex_Builds will be used instead.');
  await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());

  // The only one left: nobody to hand over to.
  await pick(user, 'Alex_Builds', 'Remove');
  dialog = screen.getByRole('dialog');
  expect(dialog).toHaveTextContent('No accounts will be left. Play needs an account.');
});

it('does not name a replacement when the account being removed is not in use', async () => {
  const { user } = renderApp(<Profile />);
  await pick(user, 'Alex_Builds', 'Remove');
  expect(screen.getByRole('dialog')).not.toHaveTextContent(/will be used instead|No accounts will be left/);
});
