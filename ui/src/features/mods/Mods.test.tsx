import { Webview } from '@tauri-apps/api/webview';
import { act, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { renderApp, sent } from '../../test/backend';
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
  expect(await screen.findByText(/Mods need Fabric or Quilt/)).toBeInTheDocument();
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
  expect(zoomify).toBeDisabled();
  await act(async () => toggle.release());
  await waitFor(() => expect(screen.getByRole('switch', { name: 'Use zoomify-2.11.2' })).toBeEnabled());

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

it('ignores a drop with no files and a drop while an add is already running', async () => {
  const { calls, backend } = renderApp(<Mods id="asalith-fields" />);
  const add = intercept(backend, 'mods_add');
  add.mode = 'hold';
  await screen.findByText(sodium);
  await dropEvent({ type: 'drop', paths: [], position: { x: 1, y: 1 } });
  expect(sent(calls, 'mods_add')).toEqual([]);
  await dropEvent({ type: 'drop', paths: ['C:/a.jar'], position: { x: 1, y: 1 } });
  await dropEvent({ type: 'drop', paths: ['C:/b.jar'], position: { x: 1, y: 1 } });
  expect(sent(calls, 'mods_add')).toEqual([{ id: 'asalith-fields', paths: ['C:/a.jar'] }]);
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
