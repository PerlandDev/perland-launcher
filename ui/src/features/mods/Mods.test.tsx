import { Webview } from '@tauri-apps/api/webview';
import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { Mods } from './Mods';

type DragHandler = Parameters<Webview['onDragDropEvent']>[0];

const failure = { category: 'game', code: 'mod_not_jar', message: 'nope', fix: null, detail: null, params: {} };
const sodium = 'sodium-fabric-0.5.8+mc1.20.1';

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

// Capture the drag-drop handler the screen registers, and hand back a spy for unlisten.
let drag: DragHandler;
let unlisten: ReturnType<typeof vi.fn>;
beforeEach(() => {
  unlisten = vi.fn();
  vi.spyOn(Webview.prototype, 'onDragDropEvent').mockImplementation((handler) => {
    drag = handler;
    return Promise.resolve(unlisten as unknown as () => void);
  });
});
const dropEvent = (payload: object) => act(() => drag({ payload } as never));

it('lists, toggles, adds and removes mods', async () => {
  const { user, calls } = renderApp(<Mods id="asalith-fields" />);
  expect(await screen.findByText('sodium-fabric-0.5.8+mc1.20.1')).toBeInTheDocument();
  expect(screen.getByText('Asalith Fields · 3 mods')).toBeInTheDocument();
  const zoomify = screen.getByRole('switch', { name: 'Use zoomify-2.11.2' });
  expect(zoomify).toHaveAttribute('aria-checked', 'false');
  await user.click(zoomify);
  expect(sent(calls, 'mod_set_enabled')).toEqual([{ id: 'asalith-fields', file: 'zoomify-2.11.2.jar.disabled', enabled: true }]);

  await user.click(screen.getByRole('button', { name: 'Add mods' }));
  expect(sent(calls, 'mods_add')).toEqual([{ id: 'asalith-fields', paths: ['C:/Users/player/Downloads/lithium-fabric-0.11.2.jar'] }]);

  await user.click(await screen.findByRole('button', { name: 'Remove sodium-fabric-0.5.8+mc1.20.1' }));
  const dialog = screen.getByRole('dialog');
  await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
  expect(sent(calls, 'mod_remove')).toEqual([{ id: 'asalith-fields', file: 'sodium-fabric-0.5.8+mc1.20.1.jar' }]);
});

it('tells vanilla players they need a loader', async () => {
  renderApp(<Mods id="creative-flat" />);
  expect(await screen.findByText(/Mods need a mod loader/)).toBeInTheDocument();
});

it('says so when there are no mods yet', async () => {
  renderApp(<Mods id="creative-flat" />);
  expect(await screen.findByText(/No mods yet/)).toBeInTheDocument();
});

it('keeps keyboard focus on a mod switch after it is toggled', async () => {
  const { user } = renderApp(<Mods id="asalith-fields" />);
  const zoomify = await screen.findByRole('switch', { name: 'Use zoomify-2.11.2' });
  zoomify.focus();
  await user.keyboard('{Enter}');
  await waitFor(() => expect(screen.getByRole('switch', { name: 'Use zoomify-2.11.2' })).toHaveAttribute('aria-checked', 'true'));
  expect(screen.getByRole('switch', { name: 'Use zoomify-2.11.2' })).toHaveFocus();
});

it('sends one toggle for a double click and shows a failed toggle', async () => {
  const { user, calls, backend } = renderApp(<Mods id="asalith-fields" />);
  const toggle = intercept(backend, 'mod_set_enabled');
  toggle.mode = 'hold';
  const zoomify = await screen.findByRole('switch', { name: 'Use zoomify-2.11.2' });
  await user.dblClick(zoomify);
  expect(sent(calls, 'mod_set_enabled')).toHaveLength(1);
  expect(zoomify).toHaveAttribute('aria-disabled', 'true');
  await act(async () => toggle.release());
  await waitFor(() => expect(screen.getByRole('switch', { name: 'Use zoomify-2.11.2' })).not.toHaveAttribute('aria-disabled'));

  toggle.mode = 'fail';
  await user.click(screen.getByRole('switch', { name: 'Use sodium-fabric-0.5.8+mc1.20.1' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Only .jar files can be added as mods.');
});

it('does nothing when the file dialog is cancelled', async () => {
  const { user, calls, backend } = renderApp(<Mods id="asalith-fields" />);
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'plugin:dialog|open' ? Promise.resolve(null) : handle(cmd, args)));
  await user.click(await screen.findByRole('button', { name: 'Add mods' }));
  await waitFor(() => expect(screen.getByRole('button', { name: 'Add mods' })).toBeEnabled());
  expect(sent(calls, 'mods_add')).toEqual([]);
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('shows a failure to open the file dialog instead of leaving it unhandled', async () => {
  const { user, calls, backend } = renderApp(<Mods id="asalith-fields" />);
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'plugin:dialog|open' ? Promise.reject(new Error('no dialog')) : handle(cmd, args)));
  await user.click(await screen.findByRole('button', { name: 'Add mods' }));
  expect(await screen.findByRole('alert')).toBeInTheDocument();
  expect(sent(calls, 'mods_add')).toEqual([]);
  expect(screen.getByRole('button', { name: 'Add mods' })).toBeEnabled();
});

it('sends one add for a double click and disables Add mods while it runs', async () => {
  const { user, calls, backend } = renderApp(<Mods id="asalith-fields" />);
  const add = intercept(backend, 'mods_add');
  add.mode = 'hold';
  await user.dblClick(await screen.findByRole('button', { name: 'Add mods' }));
  await waitFor(() => expect(sent(calls, 'mods_add')).toHaveLength(1));
  expect(screen.getByRole('button', { name: 'Add mods' })).toBeDisabled();
  await act(async () => add.release());
  expect(await screen.findByText('lithium-fabric-0.11.2')).toBeInTheDocument();
  expect(sent(calls, 'mods_add')).toHaveLength(1);
});

it('shows a failed add in the screen', async () => {
  const { user, backend } = renderApp(<Mods id="asalith-fields" />);
  intercept(backend, 'mods_add').mode = 'fail';
  await user.click(await screen.findByRole('button', { name: 'Add mods' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Only .jar files can be added as mods.');
});

it('adds dropped files and shows the drop target while dragging', async () => {
  const { calls } = renderApp(<Mods id="asalith-fields" />);
  await screen.findByText(sodium);
  await dropEvent({ type: 'enter', paths: ['C:/a.jar'], position: { x: 1, y: 1 } });
  expect(screen.getByText('Drop .jar files to add them')).toBeInTheDocument();
  await dropEvent({ type: 'leave' });
  expect(screen.queryByText('Drop .jar files to add them')).not.toBeInTheDocument();
  await dropEvent({ type: 'over', position: { x: 1, y: 1 } });
  await dropEvent({ type: 'drop', paths: ['C:/Users/player/Downloads/a.jar'], position: { x: 1, y: 1 } });
  expect(screen.queryByText('Drop .jar files to add them')).not.toBeInTheDocument();
  await waitFor(() => expect(sent(calls, 'mods_add')).toEqual([{ id: 'asalith-fields', paths: ['C:/Users/player/Downloads/a.jar'] }]));
  expect(await screen.findByText('a')).toBeInTheDocument();
});

it('ignores a drop with no files, and tells the player when a drop arrives while an add is running', async () => {
  const { calls, backend } = renderApp(<Mods id="asalith-fields" />);
  const add = intercept(backend, 'mods_add');
  add.mode = 'hold';
  await screen.findByText(sodium);
  await dropEvent({ type: 'drop', paths: [], position: { x: 1, y: 1 } });
  expect(sent(calls, 'mods_add')).toEqual([]);
  await dropEvent({ type: 'drop', paths: ['C:/a.jar'], position: { x: 1, y: 1 } });
  await dropEvent({ type: 'drop', paths: ['C:/b.jar'], position: { x: 1, y: 1 } });
  expect(sent(calls, 'mods_add')).toEqual([{ id: 'asalith-fields', paths: ['C:/a.jar'] }]);
  expect(screen.getByRole('status')).toHaveTextContent('Still adding the last files. Drop these again in a moment.');
  await act(async () => add.release());
});

it('stops listening for drops when the screen goes away', async () => {
  const { unmount } = renderApp(<Mods id="asalith-fields" />);
  await screen.findByText(sodium);
  await waitFor(() => expect(Webview.prototype.onDragDropEvent).toHaveBeenCalled());
  await act(async () => {});
  unmount();
  expect(unlisten).toHaveBeenCalledTimes(1);
});

it('stops listening even if the screen goes away before the listener is ready', async () => {
  let ready: (stop: () => void) => void = () => {};
  vi.spyOn(Webview.prototype, 'onDragDropEvent').mockImplementation(() => new Promise((resolve) => (ready = resolve)));
  const { unmount } = renderApp(<Mods id="asalith-fields" />);
  unmount();
  await act(async () => ready(unlisten as unknown as () => void));
  expect(unlisten).toHaveBeenCalledTimes(1);
});

it('shows a failure to set up drag and drop instead of leaving it unhandled', async () => {
  vi.spyOn(Webview.prototype, 'onDragDropEvent').mockRejectedValue(new Error('no webview'));
  renderApp(<Mods id="asalith-fields" />);
  expect(await screen.findByRole('alert')).toBeInTheDocument();
  // The rest of the screen still works.
  expect(await screen.findByText(sodium)).toBeInTheDocument();
});

it('opens the mods folder, one call at a time, and shows a failure', async () => {
  const { user, calls, backend } = renderApp(<Mods id="asalith-fields" />);
  const folder = intercept(backend, 'open_folder');
  folder.mode = 'hold';
  await user.dblClick(await screen.findByRole('button', { name: 'Open folder' }));
  expect(sent(calls, 'open_folder')).toEqual([{ target: { kind: 'mods', id: 'asalith-fields' } }]);
  expect(screen.getByRole('button', { name: 'Open folder' })).toBeDisabled();
  await act(async () => folder.release());
  await waitFor(() => expect(screen.getByRole('button', { name: 'Open folder' })).toBeEnabled());

  folder.mode = 'fail';
  await user.click(screen.getByRole('button', { name: 'Open folder' }));
  expect(await screen.findByRole('alert')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Open folder' })).toBeEnabled();
});

it('asks before removing, says what happens, and sends nothing on Cancel', async () => {
  const { user, calls } = renderApp(<Mods id="asalith-fields" />);
  await user.click(await screen.findByRole('button', { name: `Remove ${sodium}` }));
  const dialog = screen.getByRole('dialog');
  expect(within(dialog).getByRole('heading', { name: `Remove ${sodium}?` })).toBeInTheDocument();
  expect(within(dialog).getByText("The file is deleted from this instance's mods folder.")).toBeInTheDocument();
  await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(calls, 'mod_remove')).toEqual([]);
  expect(screen.getByText(sodium)).toBeInTheDocument();
});

it('sends one remove for a double click and closes the dialog when it is done', async () => {
  const { user, calls, backend } = renderApp(<Mods id="asalith-fields" />);
  const remove = intercept(backend, 'mod_remove');
  remove.mode = 'hold';
  await user.click(await screen.findByRole('button', { name: `Remove ${sodium}` }));
  const confirm = within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' });
  await user.dblClick(confirm);
  expect(sent(calls, 'mod_remove')).toHaveLength(1);
  expect(confirm).toBeDisabled();
  await act(async () => remove.release());
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(screen.queryByText(sodium)).not.toBeInTheDocument();
});

it('keeps the dialog open and shows why a remove failed inside it', async () => {
  const { user, backend } = renderApp(<Mods id="asalith-fields" />);
  intercept(backend, 'mod_remove').mode = 'fail';
  await user.click(await screen.findByRole('button', { name: `Remove ${sodium}` }));
  const dialog = screen.getByRole('dialog');
  await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
  expect(await within(dialog).findByRole('alert')).toHaveTextContent('Only .jar files can be added as mods.');
  expect(within(dialog).getByRole('button', { name: 'Remove' })).toBeEnabled();
  // Cancelling and reopening starts clean.
  await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
  await user.click(screen.getByRole('button', { name: `Remove ${sodium}` }));
  expect(within(screen.getByRole('dialog')).queryByRole('alert')).not.toBeInTheDocument();
});

it('offers Try again when the list fails to load', async () => {
  const { user, backend, qc } = renderApp(<Mods id="asalith-fields" />);
  await screen.findByText(sodium);
  const list = intercept(backend, 'mods_list');
  list.mode = 'fail';
  await act(async () => qc.resetQueries({ queryKey: ['mods', 'asalith-fields'] }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Only .jar files can be added as mods.');
  expect(screen.queryByText(/No mods yet/)).not.toBeInTheDocument();
  list.mode = 'pass';
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  expect(await screen.findByText(sodium)).toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('keeps a toggled switch focused while it is busy', async () => {
  const { user, backend } = renderApp(<Mods id="asalith-fields" />);
  intercept(backend, 'mod_set_enabled').mode = 'hold';
  const zoomify = await screen.findByRole('switch', { name: 'Use zoomify-2.11.2' });
  zoomify.focus();
  await user.keyboard('{Enter}');
  expect(screen.getByRole('switch', { name: 'Use zoomify-2.11.2' })).toHaveFocus();
  expect(screen.getByRole('switch', { name: 'Use zoomify-2.11.2' })).toHaveAttribute('aria-disabled', 'true');
});

it('sends only the .jar files of a mixed drop and names the skipped ones', async () => {
  const { calls } = renderApp(<Mods id="asalith-fields" />);
  await screen.findByText(sodium);
  await dropEvent({ type: 'drop', paths: ['C:/mods/a.jar', 'C:/mods/notes.txt', 'D:\\mods\\B.JAR', 'C:/mods/readme'], position: { x: 1, y: 1 } });
  await waitFor(() => expect(sent(calls, 'mods_add')).toEqual([{ id: 'asalith-fields', paths: ['C:/mods/a.jar', 'D:\\mods\\B.JAR'] }]));
  expect(screen.getByRole('status')).toHaveTextContent('Skipped (not a .jar): notes.txt, readme');
});

it('does not call the backend when nothing dropped is a .jar', async () => {
  const { calls } = renderApp(<Mods id="asalith-fields" />);
  await screen.findByText(sodium);
  await dropEvent({ type: 'drop', paths: ['C:/mods/notes.txt'], position: { x: 1, y: 1 } });
  expect(screen.getByRole('status')).toHaveTextContent('Skipped (not a .jar): notes.txt');
  expect(sent(calls, 'mods_add')).toEqual([]);
  // The next real add clears the note.
  await dropEvent({ type: 'drop', paths: ['C:/mods/a.jar'], position: { x: 1, y: 1 } });
  await waitFor(() => expect(sent(calls, 'mods_add')).toHaveLength(1));
  expect(screen.queryByRole('status')).not.toBeInTheDocument();
});

it.each(['toggle', 'add', 'remove'] as const)('asks for the list again after a failed %s so it never goes stale', async (action) => {
  const { user, calls, backend } = renderApp(<Mods id="asalith-fields" />);
  await screen.findByText(sodium);
  const command = { toggle: 'mod_set_enabled', add: 'mods_add', remove: 'mod_remove' }[action];
  intercept(backend, command).mode = 'fail';
  const before = sent(calls, 'mods_list').length;
  if (action === 'toggle') await user.click(screen.getByRole('switch', { name: 'Use zoomify-2.11.2' }));
  if (action === 'add') await user.click(screen.getByRole('button', { name: 'Add mods' }));
  if (action === 'remove') {
    await user.click(screen.getByRole('button', { name: `Remove ${sodium}` }));
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  }
  await waitFor(() => expect(sent(calls, 'mods_list').length).toBe(before + 1));
});

it('shows a file that was added even though the add reported a failure', async () => {
  const { user, backend } = renderApp(<Mods id="asalith-fields" />);
  await screen.findByText(sodium);
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation(async (cmd, args) => {
    if (cmd !== 'mods_add') return handle(cmd, args);
    await handle(cmd, args); // the first file was copied, then the second failed
    throw failure;
  });
  await user.click(screen.getByRole('button', { name: 'Add mods' }));
  expect(await screen.findByText('lithium-fabric-0.11.2')).toBeInTheDocument();
  expect(screen.getByRole('alert')).toBeInTheDocument();
});

it('shows both rows when a mod exists as a .jar and a .jar.disabled twin', async () => {
  const error = vi.spyOn(console, 'error').mockImplementation(() => {});
  const { qc, backend } = renderApp(<Mods id="asalith-fields" />);
  await screen.findByText(sodium);
  backend.state.mods.get('asalith-fields')!.push({ fileName: 'sodium-fabric-0.5.8+mc1.20.1.jar.disabled', enabled: false, size: 1000 });
  await act(async () => qc.invalidateQueries({ queryKey: ['mods', 'asalith-fields'] }));
  await waitFor(() => expect(screen.getAllByText(sodium)).toHaveLength(2));
  expect(screen.getAllByRole('switch', { name: `Use ${sodium}` })).toHaveLength(2);
  expect(error).not.toHaveBeenCalled();
});

it('refreshes the list when the window regains focus, and stops after the screen goes away', async () => {
  const { calls, unmount } = renderApp(<Mods id="asalith-fields" />);
  await screen.findByText(sodium);
  const before = sent(calls, 'mods_list').length;
  await act(async () => {
    window.dispatchEvent(new Event('focus'));
  });
  await waitFor(() => expect(sent(calls, 'mods_list').length).toBe(before + 1));
  unmount();
  window.dispatchEvent(new Event('focus'));
  await act(async () => {});
  expect(sent(calls, 'mods_list').length).toBe(before + 1);
});

describe('a pack instance', () => {
  it("lists the pack's mods read-only and the player's additions with their actions", async () => {
    const { user, calls } = renderApp(<Mods id="sky-islands" />);
    expect(await screen.findByRole('heading', { name: 'From the pack' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Your additions' })).toBeInTheDocument();
    expect(screen.getByText('Create')).toBeInTheDocument();
    expect(screen.queryByRole('switch', { name: 'Use Create' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Remove Create' })).not.toBeInTheDocument();
    expect(screen.getByText('Optional mods change the next time you play.')).toBeInTheDocument();
    await user.click(screen.getByRole('switch', { name: 'Use Iris Shaders' }));
    expect(sent(calls, 'pack_set_optional')).toEqual([{ id: 'sky-islands', path: 'mods/iris.jar', enabled: true }]);
    expect(screen.getByRole('button', { name: 'Remove zoomify-2.11.2' })).toBeInTheDocument();
  });

  it('shows nothing removable until it knows the instance is a pack', async () => {
    const backend = createMockBackend();
    let releaseListing = () => {};
    renderWithProviders(<Mods id="sky-islands" />, {
      ipc: (cmd, args) => {
        // The folder holds the pack's own jar; the instance listing is slow to arrive.
        if (cmd === 'mods_list') return [{ fileName: 'create-0.5.1.jar', enabled: true, size: 1024 }];
        if (cmd === 'instances_list') return new Promise((resolve) => (releaseListing = () => resolve(backend.handle(cmd, args))));
        return backend.handle(cmd, args);
      },
    });
    expect(await screen.findByText('Loading…')).toBeInTheDocument();
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(screen.queryByText('create-0.5.1')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Remove / })).not.toBeInTheDocument();
    releaseListing();
    expect(await screen.findByRole('heading', { name: 'From the pack' })).toBeInTheDocument();
    expect(screen.queryByText('create-0.5.1')).not.toBeInTheDocument();
  });

  it("asks for the pack's lists again after a failed change", async () => {
    const { user, calls, backend } = renderApp(<Mods id="sky-islands" />);
    const zoomify = await screen.findByRole('switch', { name: 'Use zoomify-2.11.2' });
    intercept(backend, 'mod_set_enabled').mode = 'fail';
    const before = sent(calls, 'pack_contents').length;
    await user.click(zoomify);
    await waitFor(() => expect(sent(calls, 'pack_contents').length).toBe(before + 1));
  });
});

it('locks toggles and removal while the game runs, but still lets players add mods', async () => {
  const { user, calls } = renderApp(<Mods id="asalith-fields" />);
  const zoomify = await screen.findByRole('switch', { name: 'Use zoomify-2.11.2' });
  expect(screen.queryByText(/Stop the game to turn mods on or off/)).not.toBeInTheDocument();

  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(screen.getByText(/Stop the game to turn mods on or off or remove them/)).toBeInTheDocument();
  expect(zoomify).toHaveAttribute('aria-disabled', 'true');
  const remove = screen.getByRole('button', { name: 'Remove sodium-fabric-0.5.8+mc1.20.1' });
  expect(remove).toHaveAttribute('aria-disabled', 'true');

  await user.click(zoomify);
  await user.click(remove);
  expect(sent(calls, 'mod_set_enabled')).toEqual([]);
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Add mods' }));
  expect(sent(calls, 'mods_add')).toHaveLength(1);

  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'idle', exit: null, update: null }));
  expect(screen.queryByText(/Stop the game to turn mods on or off/)).not.toBeInTheDocument();
  expect(screen.getByRole('switch', { name: 'Use zoomify-2.11.2' })).not.toHaveAttribute('aria-disabled');
  await user.click(screen.getByRole('switch', { name: 'Use zoomify-2.11.2' }));
  expect(sent(calls, 'mod_set_enabled')).toHaveLength(1);
});

it('finds mods for a modded instance only', async () => {
  const { user, unmount } = renderApp(<Mods id="asalith-fields" />);
  await user.click(await screen.findByRole('button', { name: 'Find mods' }));
  expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'mod', instance: 'asalith-fields' });
  unmount();
  renderApp(<Mods id="creative-flat" />);
  await screen.findByText(/Mods need a mod loader/);
  expect(screen.queryByRole('button', { name: 'Find mods' })).not.toBeInTheDocument();
});

it('points vanilla players to modpacks from the loader note, and not on a modded instance', async () => {
  const { user, unmount } = renderApp(<Mods id="creative-flat" />);
  await screen.findByText(/Mods need a mod loader/);
  await user.click(screen.getByRole('button', { name: 'Browse modpacks' }));
  expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'modpack' });
  unmount();
  renderApp(<Mods id="asalith-fields" />);
  await screen.findByText('sodium-fabric-0.5.8+mc1.20.1');
  expect(screen.queryByRole('button', { name: 'Browse modpacks' })).not.toBeInTheDocument();
});

describe('mod icons', () => {
  const rowOf = (name: string) => screen.getByText(name).closest('li') as HTMLElement;
  const iconOf = (name: string) => rowOf(name).querySelector('.content-icon img');

  it("shows each mod's own icon, and the package glyph for a mod without one", async () => {
    const { calls } = renderApp(<Mods id="asalith-fields" />);
    await screen.findByText(sodium);
    await waitFor(() => expect(iconOf(sodium)).toHaveAttribute('src', expect.stringMatching(/^data:image\//)));
    expect(iconOf('fabric-api-0.92.2+1.20.1')).not.toBeNull();
    // The mock has no icon for a mod named with a z: it keeps the glyph.
    await waitFor(() => expect(sent(calls, 'mod_icon')).toContainEqual({ id: 'asalith-fields', file: 'zoomify-2.11.2.jar.disabled' }));
    expect(iconOf('zoomify-2.11.2')).toBeNull();
    expect(rowOf('zoomify-2.11.2').querySelector('.content-icon svg')).not.toBeNull();
  });

  it('falls back to the glyph when an icon fails to decode', async () => {
    renderApp(<Mods id="asalith-fields" />);
    await screen.findByText(sodium);
    const img = await waitFor(() => {
      const found = iconOf(sodium);
      expect(found).not.toBeNull();
      return found as HTMLImageElement;
    });
    fireEvent.error(img);
    expect(iconOf(sodium)).toBeNull();
    expect(rowOf(sodium).querySelector('.content-icon svg')).not.toBeNull();
  });

  it('asks for an icon only once its row is on screen', async () => {
    const observers: { callback: IntersectionObserverCallback; el?: Element }[] = [];
    vi.stubGlobal(
      'IntersectionObserver',
      class {
        entry: (typeof observers)[number];
        constructor(callback: IntersectionObserverCallback) {
          this.entry = { callback };
          observers.push(this.entry);
        }
        observe(el: Element) {
          this.entry.el = el;
        }
        disconnect() {}
      },
    );
    try {
      const { calls } = renderApp(<Mods id="asalith-fields" />);
      await screen.findByText(sodium);
      await new Promise((resolve) => setTimeout(resolve, 30));
      expect(sent(calls, 'mod_icon')).toEqual([]);
      const row = observers.find((o) => o.el && rowOf(sodium).contains(o.el));
      expect(row).toBeDefined();
      act(() => row!.callback([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver));
      await waitFor(() => expect(sent(calls, 'mod_icon')).toEqual([{ id: 'asalith-fields', file: `${sodium}.jar` }]));
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("shows a pack's own mods with their icons too", async () => {
    const { calls } = renderApp(<Mods id="sky-islands" />);
    expect(await screen.findByRole('heading', { name: 'From the pack' })).toBeInTheDocument();
    await waitFor(() => expect(iconOf('Create')).not.toBeNull());
    expect(sent(calls, 'mod_icon')).toContainEqual({ id: 'sky-islands', file: 'create.jar' });
  });
});
