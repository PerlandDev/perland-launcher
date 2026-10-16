import { act, screen, waitFor, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { renderApp, sent } from '../../test/backend';
import { Accounts } from './Accounts';

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

it('switches, adds and removes offline accounts', async () => {
  const { user, calls } = renderApp(<Accounts />);
  const alex = (await screen.findByText('Alex_Builds')).closest('li')!;
  expect(within(screen.getByText('Steve').closest('li')!).getByText('In use')).toBeInTheDocument();
  await user.click(within(alex).getByRole('button', { name: 'Use' }));
  expect(sent(calls, 'account_set_active')).toHaveLength(1);

  const input = screen.getByRole('textbox', { name: 'Player name' });
  await user.type(input, 'no spaces!');
  await user.click(screen.getByRole('button', { name: 'Add offline account' }));
  expect(screen.getByText('Player names must be 3–16 letters, numbers or underscores.')).toBeInTheDocument();
  expect(sent(calls, 'account_add_offline')).toEqual([]);
  await user.clear(input);
  await user.type(input, 'Notch');
  await user.click(screen.getByRole('button', { name: 'Add offline account' }));
  expect(sent(calls, 'account_add_offline')).toEqual([{ name: 'Notch' }]);

  await user.click(within(alex).getByRole('button', { name: 'Remove Alex_Builds' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  expect(sent(calls, 'account_remove')).toHaveLength(1);
});

it('keeps Microsoft sign-in as coming soon and locks offline in release builds', async () => {
  renderApp(<Accounts />, { release: true });
  expect(await screen.findByText(/Offline accounts unlock after you sign in once/)).toBeInTheDocument();
  expect(screen.queryByRole('textbox', { name: 'Player name' })).not.toBeInTheDocument();
  expect(screen.getByText('Sign in with Microsoft').closest('[aria-disabled]')).toHaveAttribute('aria-disabled', 'true');
});

it('does not say the name is wrong until the player has typed it and tried to add it', async () => {
  const { user } = renderApp(<Accounts />);
  const input = await screen.findByRole('textbox', { name: 'Player name' });
  expect(screen.queryByText(rule)).not.toBeInTheDocument();
  await user.type(input, 'ab');
  expect(screen.queryByText(rule)).not.toBeInTheDocument();
  expect(input).not.toHaveAttribute('aria-invalid');

  await user.click(screen.getByRole('button', { name: 'Add offline account' }));
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
  const { user } = renderApp(<Accounts />);
  const input = await screen.findByRole('textbox', { name: 'Player name' });
  await user.click(input);
  await user.tab();
  expect(screen.queryByText(rule)).not.toBeInTheDocument();
  await user.click(input);
  await user.type(input, 'a b');
  await user.tab();
  expect(screen.getByText(rule)).toBeInTheDocument();
});

it('sends one switch for a double click and says who is in use now', async () => {
  const { user, calls, backend } = renderApp(<Accounts />);
  const use = intercept(backend, 'account_set_active');
  use.mode = 'hold';
  const alex = (await screen.findByText('Alex_Builds')).closest('li')!;
  await user.dblClick(within(alex).getByRole('button', { name: 'Use' }));
  expect(sent(calls, 'account_set_active')).toHaveLength(1);
  await act(async () => use.release());
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Alex_Builds is now in use.'));
  expect(within(screen.getByText('Alex_Builds').closest('li')!).getByText('In use')).toBeInTheDocument();
});

it('asks for the list again after a failed switch so it never goes stale', async () => {
  const { user, calls, backend } = renderApp(<Accounts />);
  const alex = (await screen.findByText('Alex_Builds')).closest('li')!;
  intercept(backend, 'account_set_active').mode = 'fail';
  const before = sent(calls, 'accounts_list').length;
  await user.click(within(alex).getByRole('button', { name: 'Use' }));
  await waitFor(() => expect(sent(calls, 'accounts_list').length).toBe(before + 1));
  expect(screen.getByRole('status')).toBeEmptyDOMElement();
});

it('sends one add for a double click, clears the field, and shows a failed add under the form', async () => {
  const { user, calls, backend } = renderApp(<Accounts />);
  const add = intercept(backend, 'account_add_offline');
  const input = await screen.findByRole('textbox', { name: 'Player name' });
  await user.type(input, 'Notch');
  add.mode = 'hold';
  await user.dblClick(screen.getByRole('button', { name: 'Add offline account' }));
  expect(sent(calls, 'account_add_offline')).toHaveLength(1);
  await act(async () => add.release());
  await waitFor(() => expect(input).toHaveValue(''));
  expect(await screen.findByText('Notch')).toBeInTheDocument();

  add.mode = 'fail';
  await user.type(input, 'Herobrine');
  await user.click(screen.getByRole('button', { name: 'Add offline account' }));
  expect(await screen.findByRole('alert')).toHaveTextContent(diskIo);
  expect(input).toHaveValue('Herobrine');
});

it('keeps the add button focused while an add is in flight', async () => {
  const { user, backend } = renderApp(<Accounts />);
  intercept(backend, 'account_add_offline').mode = 'hold';
  await user.type(await screen.findByRole('textbox', { name: 'Player name' }), 'Notch');
  const button = screen.getByRole('button', { name: 'Add offline account' });
  button.focus();
  await user.keyboard('{Enter}');
  expect(button).toHaveFocus();
  expect(button).not.toBeDisabled();
  expect(button).toHaveAttribute('aria-disabled', 'true');
});

it('says what removing loses and that it can be added again, and sends one remove for a double click', async () => {
  const { user, calls, backend } = renderApp(<Accounts />);
  const remove = intercept(backend, 'account_remove');
  remove.mode = 'hold';
  await user.click(await screen.findByRole('button', { name: 'Remove Alex_Builds' }));
  const dialog = screen.getByRole('dialog', { name: 'Remove Alex_Builds?' });
  expect(dialog).toHaveTextContent('You can add it again any time. Your worlds stay where they are.');
  await user.dblClick(within(dialog).getByRole('button', { name: 'Remove' }));
  expect(sent(calls, 'account_remove')).toHaveLength(1);
  await act(async () => remove.release());
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(screen.queryByText('Alex_Builds')).not.toBeInTheDocument();
});

it('does not promise an offline account can be added again when offline accounts are locked', async () => {
  const { user } = renderApp(<Accounts />, { release: true });
  await user.click(await screen.findByRole('button', { name: 'Remove Alex_Builds' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog).not.toHaveTextContent('add it again any time');
  expect(dialog).toHaveTextContent('add offline accounts again after you sign in with Microsoft, which is coming soon');
});

it('keeps the dialog open and shows why a remove failed inside it', async () => {
  const { user, calls, backend } = renderApp(<Accounts />);
  await screen.findByText('Alex_Builds');
  intercept(backend, 'account_remove').mode = 'fail';
  const before = sent(calls, 'accounts_list').length;
  await user.click(screen.getByRole('button', { name: 'Remove Alex_Builds' }));
  const dialog = screen.getByRole('dialog');
  await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
  expect(await within(dialog).findByRole('alert')).toHaveTextContent(diskIo);
  await waitFor(() => expect(sent(calls, 'accounts_list').length).toBe(before + 1));
  // Cancelling and reopening starts clean.
  await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
  await user.click(screen.getByRole('button', { name: 'Remove Alex_Builds' }));
  expect(within(screen.getByRole('dialog')).queryByRole('alert')).not.toBeInTheDocument();
});

it('offers Try again when the list fails to load', async () => {
  const { user, backend, qc } = renderApp(<Accounts />);
  await screen.findByText('Steve');
  const list = intercept(backend, 'accounts_list');
  list.mode = 'fail';
  await act(async () => qc.resetQueries({ queryKey: ['accounts'] }));
  expect(await screen.findByRole('alert')).toHaveTextContent(diskIo);
  expect(screen.queryByText('No accounts yet.')).not.toBeInTheDocument();
  list.mode = 'pass';
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  expect(await screen.findByText('Steve')).toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('says nothing about adding it back while it is still unknown whether offline accounts are allowed', async () => {
  const { user, backend, qc } = renderApp(<Accounts />);
  await screen.findByText('Steve');
  const info = intercept(backend, 'app_info');
  info.mode = 'hold';
  act(() => {
    void qc.resetQueries({ queryKey: ['appInfo'] }); // parked, so the answer never arrives
  });
  await user.click(screen.getByRole('button', { name: 'Remove Alex_Builds' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog).toHaveTextContent('Your worlds stay where they are.');
  expect(dialog).not.toHaveTextContent(/add it again|Microsoft/);
});

it('moves focus to the next account after a remove, to the previous one at the end, and to the add field when none are left', async () => {
  const { user } = renderApp(<Accounts />);
  const confirm = async (name: string) => {
    await user.click(await screen.findByRole('button', { name: `Remove ${name}` }));
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  };
  await confirm('Steve');
  expect(screen.getByRole('button', { name: 'Remove Alex_Builds' })).toHaveFocus();
  await confirm('Alex_Builds');
  expect(screen.getByRole('textbox', { name: 'Player name' })).toHaveFocus();
});

it('moves focus to the previous account when the last one is removed', async () => {
  const { user } = renderApp(<Accounts />);
  await user.click(await screen.findByRole('button', { name: 'Remove Alex_Builds' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  await waitFor(() => expect(screen.getByRole('button', { name: 'Remove Steve' })).toHaveFocus());
});

it('moves focus to the add section when no accounts are left and offline accounts are locked', async () => {
  const { user } = renderApp(<Accounts />, { release: true });
  await user.click(await screen.findByRole('button', { name: 'Remove Alex_Builds' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  await user.click(await screen.findByRole('button', { name: 'Remove Steve' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  await waitFor(() => expect(screen.getByRole('heading', { name: 'Add offline account' })).toHaveFocus());
});

it('announces the new active account after an add and after removing the active one', async () => {
  const { user } = renderApp(<Accounts />);
  const input = await screen.findByRole('textbox', { name: 'Player name' });
  await user.type(input, 'Notch');
  await user.click(screen.getByRole('button', { name: 'Add offline account' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Notch is now in use.'));

  // Removing an account that is not in use changes nothing about who is.
  await user.click(screen.getByRole('button', { name: 'Remove Steve' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(screen.getByRole('status')).toHaveTextContent('Notch is now in use.');

  // Removing the one in use hands over to the first one left.
  await user.click(screen.getByRole('button', { name: 'Remove Notch' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Alex_Builds is now in use.'));
});

it('clears the announcement when removing the last account leaves nobody in use', async () => {
  const { user } = renderApp(<Accounts />);
  const alex = (await screen.findByText('Alex_Builds')).closest('li')!;
  await user.click(within(alex).getByRole('button', { name: 'Use' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Alex_Builds is now in use.'));
  await user.click(screen.getByRole('button', { name: 'Remove Steve' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  await user.click(await screen.findByRole('button', { name: 'Remove Alex_Builds' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(screen.getByRole('status')).toBeEmptyDOMElement();
});

it('names who will be used instead when the account in use is being removed', async () => {
  const { user } = renderApp(<Accounts />);
  // Steve is in use; Alex_Builds is the first one left.
  await user.click(await screen.findByRole('button', { name: 'Remove Steve' }));
  let dialog = screen.getByRole('dialog');
  expect(dialog).toHaveTextContent('Alex_Builds will be used instead.');
  await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());

  // The only one left: nobody to hand over to.
  await user.click(screen.getByRole('button', { name: 'Remove Alex_Builds' }));
  dialog = screen.getByRole('dialog');
  expect(dialog).toHaveTextContent('No accounts will be left. Play needs an account.');
});

it('does not name a replacement when the account being removed is not in use', async () => {
  const { user } = renderApp(<Accounts />);
  await user.click(await screen.findByRole('button', { name: 'Remove Alex_Builds' }));
  expect(screen.getByRole('dialog')).not.toHaveTextContent(/will be used instead|No accounts will be left/);
});

it('shows the time played in every instance together', async () => {
  renderApp(<Accounts />, { playtime: { 'asalith-fields': 12 * 3600, 'creative-flat': 30 * 60 } });
  expect(await screen.findByText('Time played on this computer: 12 h 30 min')).toBeInTheDocument();
});

it('leaves the total out before a minute is played', async () => {
  const { calls } = renderApp(<Accounts />, { playtime: { 'asalith-fields': 59 } });
  await screen.findByText('Steve');
  await waitFor(() => expect(sent(calls, 'playtime_list')).toHaveLength(1));
  await act(async () => {});
  expect(screen.queryByText(/Time played/)).toBeNull();
});
