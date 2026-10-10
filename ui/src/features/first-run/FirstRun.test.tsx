import { act, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { FirstRun } from './FirstRun';

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

it('walks a new player from welcome to their first instance', async () => {
  const { user, calls } = renderApp(<FirstRun />, { firstRun: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'Español' }));
  expect(await screen.findByRole('heading', { name: '¿Quién va a jugar?' })).toBeInTheDocument();
  await user.type(screen.getByRole('textbox', { name: 'Nombre de jugador' }), 'Steve');
  await user.click(screen.getByRole('button', { name: 'Continuar' }));
  expect(sent(calls, 'account_add_offline')).toEqual([{ name: 'Steve' }]);
  expect(await screen.findByRole('heading', { name: 'Tu primera instancia' })).toBeInTheDocument();
  const finish = await screen.findByRole('button', { name: 'A jugar' });
  await vi.waitFor(() => expect(finish).toBeEnabled()); // the version list has loaded
  await user.click(finish);
  await vi.waitFor(() => expect(routeSnapshot()).toMatchObject({ name: 'home', id: 'minecraft-26-3' }));
  expect(sent(calls, 'instance_create')).toEqual([{ request: { name: 'Minecraft 26.3', minecraft: '26.3', loader: null } }]);
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { firstRunDone: true, language: 'es' } });
});

it('lets release builds skip the account step honestly', async () => {
  const { user } = renderApp(<FirstRun />, { firstRun: true, release: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  expect(await screen.findByText(/Microsoft sign-in is coming soon/)).toBeInTheDocument();
  expect(screen.getByText('Sign in with Microsoft').closest('[aria-disabled]')).toHaveAttribute('aria-disabled', 'true');
  await user.click(screen.getByRole('button', { name: 'Skip for now' }));
  expect(await screen.findByRole('heading', { name: 'Your first instance' })).toBeInTheDocument();
});

it('does not suggest the game itself is free', async () => {
  renderApp(<FirstRun />, { firstRun: true });
  expect(await screen.findByText('A free launcher for Minecraft, ready in a few clicks.')).toBeInTheDocument();
});

it('leaves focus alone on the first screen and moves it to each new step heading', async () => {
  const { user } = renderApp(<FirstRun />, { firstRun: true });
  const start = await screen.findByRole('button', { name: "Let's go" });
  expect(document.body).toHaveFocus();
  await user.click(start);
  expect(screen.getByRole('heading', { name: 'Pick your language' })).toHaveFocus();
  await user.click(screen.getByRole('button', { name: 'English' }));
  expect(await screen.findByRole('heading', { name: "Who's playing?" })).toHaveFocus();
});

it('announces an invalid name by moving focus to the field and sends nothing', async () => {
  const { user, calls } = renderApp(<FirstRun />, { firstRun: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  const input = await screen.findByRole('textbox', { name: 'Player name' });
  await user.type(input, 'no spaces!');
  await user.click(screen.getByRole('button', { name: 'Continue' }));
  expect(input).toHaveFocus();
  expect(input).toHaveAccessibleDescription(rule);
  expect(input).toHaveAttribute('aria-invalid', 'true');
  expect(sent(calls, 'account_add_offline')).toHaveLength(0);
});

it('says why a failed add failed, then sends one add for a double click on Continue', async () => {
  const { user, calls, backend } = renderApp(<FirstRun />, { firstRun: true });
  const add = intercept(backend, 'account_add_offline');
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  await user.type(await screen.findByRole('textbox', { name: 'Player name' }), 'Notch');
  add.mode = 'fail';
  await user.click(screen.getByRole('button', { name: 'Continue' }));
  expect(await screen.findByRole('alert')).toHaveTextContent(diskIo);
  expect(screen.getByRole('heading', { name: "Who's playing?" })).toBeInTheDocument();

  add.mode = 'hold';
  await user.dblClick(screen.getByRole('button', { name: 'Continue' }));
  expect(sent(calls, 'account_add_offline')).toHaveLength(2); // the failed one, then a single retry
  await act(async () => add.release());
  expect(await screen.findByRole('heading', { name: 'Your first instance' })).toBeInTheDocument();
});

it('shows neither form nor locked copy while it is unknown whether offline accounts are allowed', async () => {
  const { user, backend, qc } = renderApp(<FirstRun />, { firstRun: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  const info = intercept(backend, 'app_info');
  info.mode = 'hold';
  act(() => {
    void qc.resetQueries({ queryKey: ['appInfo'] }); // parked, so the answer never arrives
  });
  await user.click(screen.getByRole('button', { name: 'English' }));
  expect(await screen.findByRole('heading', { name: "Who's playing?" })).toBeInTheDocument();
  expect(screen.getByText('Loading…')).toBeInTheDocument();
  expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
  expect(screen.queryByText(/Microsoft sign-in is coming soon/)).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Skip for now' })).not.toBeInTheDocument();
});

it('offers Try again when the build info fails to load', async () => {
  const { user, backend, qc } = renderApp(<FirstRun />, { firstRun: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  const info = intercept(backend, 'app_info');
  info.mode = 'fail';
  await act(async () => qc.resetQueries({ queryKey: ['appInfo'] }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  expect(await screen.findByRole('alert')).toHaveTextContent(diskIo);
  expect(screen.queryByText(/Microsoft sign-in is coming soon/)).not.toBeInTheDocument();
  info.mode = 'pass';
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  expect(await screen.findByRole('textbox', { name: 'Player name' })).toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('never creates a second instance when saving the settings has to be retried', async () => {
  const { user, calls, backend } = renderApp(<FirstRun />, { firstRun: true, release: true });
  const save = intercept(backend, 'settings_set');
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  await user.click(await screen.findByRole('button', { name: 'Skip for now' }));
  const finish = await screen.findByRole('button', { name: 'Start playing' });
  await vi.waitFor(() => expect(finish).toBeEnabled());

  save.mode = 'fail';
  await user.click(finish);
  expect(await screen.findByRole('alert')).toHaveTextContent(diskIo);
  expect(routeSnapshot()).not.toMatchObject({ name: 'home', id: 'minecraft-26-3' });

  save.mode = 'pass';
  await user.click(screen.getByRole('button', { name: 'Start playing' }));
  await waitFor(() => expect(routeSnapshot()).toMatchObject({ name: 'home', id: 'minecraft-26-3' }));
  expect(sent(calls, 'instance_create')).toHaveLength(1);
  expect(sent(calls, 'settings_set')).toHaveLength(2);
});

it('sends one create for a double click on the finish button and keeps its focus', async () => {
  const { user, calls, backend } = renderApp(<FirstRun />, { firstRun: true, release: true });
  const create = intercept(backend, 'instance_create');
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  await user.click(await screen.findByRole('button', { name: 'Skip for now' }));
  const finish = await screen.findByRole('button', { name: 'Start playing' });
  await vi.waitFor(() => expect(finish).toBeEnabled());
  create.mode = 'hold';
  finish.focus();
  await user.dblClick(finish);
  expect(sent(calls, 'instance_create')).toHaveLength(1);
  expect(finish).toHaveFocus();
  await act(async () => create.release());
  await waitFor(() => expect(routeSnapshot()).toMatchObject({ name: 'home' }));
});

it('says which language is selected', async () => {
  const { user } = renderApp(<FirstRun />, { firstRun: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  expect(screen.getByRole('button', { name: 'English' })).toHaveAttribute('aria-pressed', 'true');
  expect(screen.getByRole('button', { name: 'Español' })).toHaveAttribute('aria-pressed', 'false');
});

it('offers Try again on the instance step when the version list fails, without picking a version', async () => {
  const { user, backend, qc } = renderApp(<FirstRun />, { firstRun: true, release: true });
  const versions = intercept(backend, 'catalog_minecraft');
  versions.mode = 'fail';
  await act(async () => qc.resetQueries({ queryKey: ['catalog', 'minecraft'] }));
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  await user.click(await screen.findByRole('button', { name: 'Skip for now' }));
  expect(await screen.findByRole('alert')).toHaveTextContent(diskIo);
  expect(screen.getAllByRole('alert')).toHaveLength(1);
  expect(screen.getByRole('button', { name: 'Start playing' })).toBeDisabled();

  versions.mode = 'pass';
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  await waitFor(() => expect(screen.getByRole('button', { name: 'Start playing' })).toBeEnabled());
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('shows one error card, not two, when the version list fails while picking a version', async () => {
  const { user, backend, qc } = renderApp(<FirstRun />, { firstRun: true, release: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  await user.click(await screen.findByRole('button', { name: 'Skip for now' }));
  await user.click(await screen.findByRole('button', { name: 'Pick a version' }));
  intercept(backend, 'catalog_minecraft').mode = 'fail';
  await act(async () => qc.resetQueries({ queryKey: ['catalog', 'minecraft'] }));
  expect(await screen.findByRole('alert')).toHaveTextContent(diskIo);
  expect(screen.getAllByRole('alert')).toHaveLength(1);
  expect(screen.getAllByRole('button', { name: 'Try again' })).toHaveLength(1);
});
