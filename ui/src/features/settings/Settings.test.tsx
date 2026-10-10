import { fireEvent, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { toastsSnapshot } from '../../state/toasts';
import { createMockBackend } from '../../ipc/mock';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { Settings } from './Settings';

const failure = { category: 'game', code: 'internal', message: 'nope', fix: null, detail: null, params: {} };

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

/** Render with one command failing from the very first call, until `control.failing` is turned off. */
function renderFailing(command: string) {
  const backend = createMockBackend();
  const control = { failing: true };
  const result = renderWithProviders(<Settings />, {
    ipc: (cmd, args) => (cmd === command && control.failing ? Promise.reject(failure) : backend.handle(cmd, args)),
  });
  return { ...result, control };
}

const slider = () => screen.getByRole('slider', { name: 'Memory for new instances' });
const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

it('switches language, saves preferences and opens the data folder', async () => {
  const { user, calls } = renderApp(<Settings />);
  const language = await screen.findByRole('combobox', { name: 'Language' });
  await user.selectOptions(language, 'es');
  expect(await screen.findByRole('heading', { name: 'Configuración' })).toBeInTheDocument();
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { language: 'es' } });

  await user.click(screen.getByRole('switch', { name: 'Minimizar mientras juegas' }));
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { minimizeOnPlay: false } });

  await user.click(screen.getByRole('button', { name: 'Abrir carpeta' }));
  expect(sent(calls, 'open_folder')).toEqual([{ target: { kind: 'data' } }]);
  expect(screen.getByText(/No es un producto oficial de Minecraft/)).toBeInTheDocument();
});

it('follows the computer language again when "same as this computer" is picked', async () => {
  const { user, calls } = renderApp(<Settings />);
  const language = await screen.findByRole('combobox', { name: 'Language' });
  await user.selectOptions(language, 'es');
  expect(await screen.findByRole('heading', { name: 'Configuración' })).toBeInTheDocument();

  await user.selectOptions(screen.getByRole('combobox', { name: 'Idioma' }), 'system');
  expect(await screen.findByRole('heading', { name: 'Settings' })).toBeInTheDocument();
  await waitFor(() => expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { language: null } }));
});

it('sends one save for a whole drag of the memory slider', async () => {
  const { calls } = renderApp(<Settings />);
  await screen.findByRole('slider');
  fireEvent.change(slider(), { target: { value: '5120' } });
  fireEvent.change(slider(), { target: { value: '5632' } });
  fireEvent.change(slider(), { target: { value: '6144' } });
  // The thumb follows the drag at once even though nothing is sent yet.
  expect(slider()).toHaveValue('6144');
  expect(sent(calls, 'settings_set')).toHaveLength(0);

  await waitFor(() => expect(sent(calls, 'settings_set')).toHaveLength(1));
  expect(sent(calls, 'settings_set')[0]).toMatchObject({ settings: { defaultMemoryMb: 6144 } });
  await pause(600);
  expect(sent(calls, 'settings_set')).toHaveLength(1);
  expect(slider()).toHaveValue('6144');
});

it('saves a memory change that is still settling when the screen is left', async () => {
  const { calls, unmount } = renderApp(<Settings />);
  await screen.findByRole('slider');
  fireEvent.change(slider(), { target: { value: '2048' } });
  unmount();
  await waitFor(() => expect(sent(calls, 'settings_set')).toHaveLength(1));
  expect(sent(calls, 'settings_set')[0]).toMatchObject({ settings: { defaultMemoryMb: 2048 } });
});

it('sends overlapping edits one after another so an older answer never wins', async () => {
  const { user, backend, calls } = renderApp(<Settings />);
  const save = intercept(backend, 'settings_set');
  const minimize = await screen.findByRole('switch', { name: 'Minimize while playing' });
  save.mode = 'hold';
  await user.click(minimize);
  await waitFor(() => expect(sent(calls, 'settings_set')).toHaveLength(1));

  // A second edit while the first is still being saved waits its turn and carries both changes.
  await user.selectOptions(screen.getByRole('combobox', { name: 'Language' }), 'es');
  expect(sent(calls, 'settings_set')).toHaveLength(1);
  expect(screen.getByRole('heading', { name: 'Configuración' })).toBeInTheDocument();

  save.mode = 'pass';
  save.release();
  await waitFor(() => expect(sent(calls, 'settings_set')).toHaveLength(2));
  expect(sent(calls, 'settings_set')[1]).toMatchObject({ settings: { minimizeOnPlay: false, language: 'es' } });
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Guardado'));
  expect(backend.state.settings).toMatchObject({ minimizeOnPlay: false, language: 'es' });
  expect(screen.getByRole('switch', { name: 'Minimizar mientras juegas' })).toHaveAttribute('aria-checked', 'false');
});

it('shows a failed save and puts the controls back to what is stored', async () => {
  const { user, backend } = renderApp(<Settings />);
  const save = intercept(backend, 'settings_set');
  const minimize = await screen.findByRole('switch', { name: 'Minimize while playing' });
  expect(minimize).toHaveAttribute('aria-checked', 'true');
  save.mode = 'fail';

  await user.selectOptions(screen.getByRole('combobox', { name: 'Language' }), 'es');
  await waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
  expect(toastsSnapshot().at(0)?.error).toEqual(failure);
  expect(await screen.findByRole('heading', { name: 'Settings' })).toBeInTheDocument();
  expect(screen.getByRole('combobox', { name: 'Language' })).toHaveValue('system');
  expect(screen.getByRole('switch', { name: 'Minimize while playing' })).toHaveAttribute('aria-checked', 'true');
  expect(backend.state.settings).toMatchObject({ language: null, minimizeOnPlay: true });
  expect(screen.getByRole('status')).not.toHaveTextContent('Saved');
});

it('says when a save has gone through', async () => {
  const { user } = renderApp(<Settings />);
  await user.click(await screen.findByRole('switch', { name: 'Minimize while playing' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Saved'));
});

it('offers to try again when the settings cannot be loaded', async () => {
  const { user, control } = renderFailing('settings_get');
  expect(await screen.findByRole('alert')).toBeInTheDocument();
  control.failing = false;
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  expect(await screen.findByRole('combobox', { name: 'Language' })).toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('offers to try again when the app details cannot be loaded', async () => {
  const { user, control } = renderFailing('app_info');
  expect(await screen.findByRole('alert')).toBeInTheDocument();
  control.failing = false;
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  expect(await screen.findByText('Version 0.1.0 · Developer build')).toBeInTheDocument();
});

it('opens the data folder once however often the button is pressed', async () => {
  const { user, backend, calls } = renderApp(<Settings />);
  const open = intercept(backend, 'open_folder');
  open.mode = 'hold';
  const button = await screen.findByRole('button', { name: 'Open folder' });
  await user.dblClick(button);
  expect(sent(calls, 'open_folder')).toHaveLength(1);
  open.release();
  // Once it has finished the button works again.
  await waitFor(() => expect(button).toBeEnabled());
  open.mode = 'pass';
  await user.click(button);
  expect(sent(calls, 'open_folder')).toHaveLength(2);
});

it('shows it when the data folder cannot be opened', async () => {
  const { user, backend } = renderApp(<Settings />);
  intercept(backend, 'open_folder').mode = 'fail';
  await user.click(await screen.findByRole('button', { name: 'Open folder' }));
  await waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
});

it('shows the version, the data folder and the license, with links that open outside the app', async () => {
  const { user, calls } = renderApp(<Settings />);
  expect(await screen.findByText('Version 0.1.0 · Developer build')).toBeInTheDocument();
  expect(screen.getByText('C:/Users/player/AppData/Roaming/PerlandLauncher')).toBeInTheDocument();
  expect(screen.getByText('Free software under the GNU General Public License v3.')).toBeInTheDocument();
  expect(screen.queryByText(/update/i)).not.toBeInTheDocument();
  expect(screen.queryByText(/modrinth/i)).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Read the license' }));
  await waitFor(() => expect(sent(calls, 'plugin:opener|open_url')).toHaveLength(1));
  expect(sent(calls, 'plugin:opener|open_url')[0]).toMatchObject({ url: 'https://www.gnu.org/licenses/gpl-3.0.html' });
  await user.click(screen.getByRole('button', { name: 'Source code' }));
  await waitFor(() => expect(sent(calls, 'plugin:opener|open_url')).toHaveLength(2));
  expect(sent(calls, 'plugin:opener|open_url')[1]).toMatchObject({ url: 'https://github.com/PerlandDev/perland-launcher' });
});

it('shows it when a link cannot be opened', async () => {
  const { user, backend } = renderApp(<Settings />);
  intercept(backend, 'plugin:opener|open_url').mode = 'fail';
  await user.click(await screen.findByRole('button', { name: 'Read the license' }));
  await waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
});
