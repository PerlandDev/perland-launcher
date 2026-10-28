import { act, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import App from '../../App';
import { FirstRun } from './FirstRun';

const failure = { category: 'storage', code: 'disk_io', message: 'nope', fix: null, detail: null, params: {} };
const diskIo = "Couldn't read or write a launcher file.";

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
  await user.click(screen.getByRole('button', { name: 'Iniciar sesión con Microsoft' }));
  expect(await screen.findByRole('heading', { name: 'Tu primera instancia' }, { timeout: 4000 })).toBeInTheDocument();
  expect(sent(calls, 'account_sign_in_microsoft')).toEqual([{ language: 'es' }]);
  const finish = await screen.findByRole('button', { name: 'A jugar' });
  await vi.waitFor(() => expect(finish).toBeEnabled()); // the version list has loaded
  await user.click(finish);
  await vi.waitFor(() => expect(routeSnapshot()).toMatchObject({ name: 'home', id: 'minecraft-26-3' }));
  expect(sent(calls, 'instance_create')).toEqual([{ request: { name: 'Minecraft 26.3', minecraft: '26.3', loader: null } }]);
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { firstRunDone: true, language: 'es' } });
});

it.each([{}, { beta: true }, { release: true }])('asks for a Microsoft sign-in, with no name field (%o)', async (build) => {
  const { user } = renderApp(<FirstRun />, { firstRun: true, ...build });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  expect(await screen.findByRole('button', { name: 'Sign in with Microsoft' })).toBeInTheDocument();
  expect(screen.getByText('You need a Microsoft account that owns Minecraft: Java Edition.')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Skip for now' })).toBeInTheDocument();
  expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
  expect(screen.queryByText(/coming soon/i)).not.toBeInTheDocument();
});

it('offers the sign-in without waiting for the build info', async () => {
  const { user, backend, qc } = renderApp(<FirstRun />, { firstRun: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  intercept(backend, 'app_info').mode = 'hold';
  act(() => {
    void qc.resetQueries({ queryKey: ['appInfo'] }); // parked, so the answer never arrives
  });
  await user.click(screen.getByRole('button', { name: 'English' }));
  expect(await screen.findByRole('button', { name: 'Sign in with Microsoft' })).toBeInTheDocument();
  expect(screen.queryByText('Loading…')).not.toBeInTheDocument();
});

it('lets a beta build skip the sign-in to the first instance', async () => {
  const { user, calls } = renderApp(<FirstRun />, { firstRun: true, beta: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  await user.click(await screen.findByRole('button', { name: 'Skip for now' }));
  expect(await screen.findByRole('heading', { name: 'Your first instance' })).toBeInTheDocument();
  expect(sent(calls, 'account_sign_in_microsoft')).toHaveLength(0);
});

it('goes on to the first instance once the Microsoft sign-in is done', async () => {
  const { user, calls } = renderApp(<FirstRun />, { firstRun: true, beta: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  await user.click(await screen.findByRole('button', { name: 'Sign in with Microsoft' }));
  expect(await screen.findByRole('heading', { name: 'Your first instance' }, { timeout: 4000 })).toBeInTheDocument();
  expect(sent(calls, 'account_sign_in_microsoft')).toEqual([{ language: 'en' }]);
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

it('shows the chosen language pressed, not lit', async () => {
  const { user } = renderApp(<FirstRun />, { firstRun: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  const english = screen.getByRole('button', { name: 'English' });
  const spanish = screen.getByRole('button', { name: 'Español' });
  expect(english).toHaveAttribute('data-pressed', 'true');
  expect(spanish).toHaveAttribute('data-pressed', 'false');
  expect(english).not.toHaveClass('btn-primary');
  expect(spanish).not.toHaveClass('btn-primary');
});

/** Walk a release build to step 3, skipping the sign-in, in Spanish. */
async function toInstanceStep(rendered: ReturnType<typeof renderApp>) {
  const { user } = rendered;
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'Español' }));
  await user.click(await screen.findByRole('button', { name: 'Omitir por ahora' }));
  return screen.findByRole('button', { name: 'Explorar modpacks' });
}

it('finishes first run for a community modpack without creating an instance', async () => {
  const rendered = renderApp(<FirstRun />, { firstRun: true, release: true });
  const { user, calls } = rendered;
  const browse = await toInstanceStep(rendered);
  expect(browse).toBeEnabled();
  await user.click(browse);
  await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'modpack' }));
  expect(sent(calls, 'instance_create')).toHaveLength(0);
  expect(sent(calls, 'settings_set')).toHaveLength(1);
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { firstRunDone: true, language: 'es' } });
});

it('saves the settings once for a double click on Browse modpacks', async () => {
  const rendered = renderApp(<FirstRun />, { firstRun: true, release: true });
  const { user, calls, backend } = rendered;
  const save = intercept(backend, 'settings_set');
  const browse = await toInstanceStep(rendered);
  save.mode = 'hold';
  await user.dblClick(browse);
  expect(sent(calls, 'settings_set')).toHaveLength(1);
  await act(async () => save.release());
  await waitFor(() => expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'modpack' }));
  expect(sent(calls, 'instance_create')).toHaveLength(0);
});

it('keeps first run open and shows why when saving fails on the way to Browse', async () => {
  const rendered = renderApp(<FirstRun />, { firstRun: true, release: true });
  const { user, calls, backend } = rendered;
  const save = intercept(backend, 'settings_set');
  const browse = await toInstanceStep(rendered);
  save.mode = 'fail';
  await user.click(browse);
  expect(await screen.findByRole('alert')).toHaveTextContent('No se pudo leer o escribir un archivo del launcher.');
  expect(routeSnapshot()).not.toMatchObject({ name: 'browse' });
  save.mode = 'pass';
  await user.click(screen.getByRole('button', { name: 'Explorar modpacks' }));
  await waitFor(() => expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'modpack' }));
  expect(sent(calls, 'settings_set')).toHaveLength(2);
});

it('opens Browse in the app shell once first run is done', async () => {
  const rendered = renderApp(<App />, { firstRun: true, release: true });
  const browse = await toInstanceStep(rendered);
  await rendered.user.click(browse);
  expect(await screen.findByRole('heading', { name: 'Explorar' })).toBeInTheDocument();
  expect(screen.getByRole('tab', { name: 'Modpacks' })).toHaveAttribute('aria-selected', 'true');
  expect(screen.queryByRole('heading', { name: 'Tu primera instancia' })).not.toBeInTheDocument();
});
