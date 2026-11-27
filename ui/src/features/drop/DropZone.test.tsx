import { Webview } from '@tauri-apps/api/webview';
import { act, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { navigate, rememberInstance, routeSnapshot, type Route } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { Toasts } from '../../ui/Toasts';
import { Mods } from '../mods/Mods';
import { DropZone } from './DropZone';

type DragHandler = Parameters<Webview['onDragDropEvent']>[0];

let drag: DragHandler;
let unlisten: ReturnType<typeof vi.fn>;
beforeEach(() => {
  unlisten = vi.fn();
  vi.spyOn(Webview.prototype, 'onDragDropEvent').mockImplementation((handler) => {
    drag = handler;
    return Promise.resolve(unlisten as unknown as () => void);
  });
});

const at = { x: 400, y: 300 };
const enter = (paths: string[]) => act(async () => drag({ payload: { type: 'enter', paths, position: at } } as never));
const over = (position = at) => act(async () => drag({ payload: { type: 'over', position } } as never));
const leave = () => act(async () => drag({ payload: { type: 'leave' } } as never));
const drop = (paths: string[]) => act(async () => drag({ payload: { type: 'drop', paths, position: at } } as never));

const d = (name: string) => `C:/Users/player/Downloads/${name}`;

/** The window's drop target with the toasts, on `route`; `extra` renders beside it (a screen). */
async function setup(route: Route, extra: React.ReactNode = null) {
  navigate(route);
  const app = renderApp(
    <>
      {extra}
      <DropZone />
      <Toasts />
    </>,
  );
  await waitFor(() => expect(Webview.prototype.onDragDropEvent).toHaveBeenCalled());
  // The listing arrives, so the target can be named.
  await waitFor(() => expect(sent(app.calls, 'instances_list').length).toBeGreaterThan(0));
  await act(async () => {});
  return app;
}

const overlay = () => screen.queryByTestId('drop-overlay');

it('says what a drag will add, by count and kind, and where it goes', async () => {
  await setup({ name: 'home', id: 'asalith-fields' });
  await enter([d('a.jar'), d('b.jar'), d('BSL Shaders.zip')]);
  const shown = await screen.findByTestId('drop-overlay');
  expect(await within(shown).findByText('Drop to add 2 mods and 1 shader pack')).toBeInTheDocument();
  expect(within(shown).getByText('Adds to')).toBeInTheDocument();
  expect(within(shown).getByText('Asalith Fields')).toBeInTheDocument();
  expect(within(shown).getByText('To add somewhere else, hold the files over an instance in the rail.')).toBeInTheDocument();
  await leave();
  expect(overlay()).not.toBeInTheDocument();
});

it('adds a drop to the instance on screen, refreshes its Mods tab and says so with a way to see them', async () => {
  const { calls } = await setup({ name: 'mods', id: 'asalith-fields' }, <Mods id="asalith-fields" />);
  await screen.findByText('sodium-fabric-0.5.8+mc1.20.1');
  await enter([d('iris.jar'), d('notes.txt')]);
  await drop([d('iris.jar'), d('notes.txt')]);
  expect(overlay()).not.toBeInTheDocument();
  await waitFor(() => expect(sent(calls, 'drop_add')).toEqual([{ id: 'asalith-fields', paths: [d('iris.jar'), d('notes.txt')] }]));
  // The open Mods tab shows the new jar.
  expect(await screen.findByText('iris')).toBeInTheDocument();
  expect(await screen.findByText('Added 1 mod to Asalith Fields')).toBeInTheDocument();
  expect(screen.getByText('Skipped: notes.txt (not a mod or pack)')).toBeInTheDocument();
});

it("opens the tab of what was added from the notice's Show", async () => {
  const { user } = await setup({ name: 'settings' }, null);
  navigate({ name: 'home', id: 'asalith-fields' });
  await enter([d('Fresh.zip')]);
  await drop([d('Fresh.zip')]);
  await user.click(await screen.findByRole('button', { name: 'Show' }));
  expect(routeSnapshot()).toEqual({ name: 'mods', id: 'asalith-fields', tab: 'resource_pack' });
});

it('tells the player a vanilla instance skips mods and shader packs, and adds only what it can', async () => {
  const { calls } = await setup({ name: 'home', id: 'creative-flat' });
  await enter([d('iris.jar'), d('BSL Shaders.zip')]);
  const shown = await screen.findByTestId('drop-overlay');
  expect(await within(shown).findByText('Nothing here can be added')).toBeInTheDocument();
  expect(within(shown).getByText('Creative Flat is vanilla, so mods and shader packs are skipped: they need a mod loader.')).toBeInTheDocument();
  await drop([d('iris.jar'), d('BSL Shaders.zip')]);
  expect(await screen.findByText('Nothing was added to Creative Flat')).toBeInTheDocument();
  expect(screen.getByText('Skipped: iris.jar (mods need a mod loader), BSL Shaders.zip (shaders need a mod loader)')).toBeInTheDocument();
  expect(sent(calls, 'mods_list')).toEqual([]);
});

it('starts importing a modpack dropped on its own', async () => {
  const { calls } = await setup({ name: 'home', id: 'asalith-fields' });
  await enter([d('Sky Islands 1.0.4.mrpack')]);
  expect(await screen.findByText('Drop to import a modpack')).toBeInTheDocument();
  await drop([d('Sky Islands 1.0.4.mrpack')]);
  await waitFor(() => expect(routeSnapshot()).toEqual({ name: 'newInstance', file: d('Sky Islands 1.0.4.mrpack') }));
  expect(sent(calls, 'drop_add')).toEqual([]);
});

it('asks which instance when the screen is about none, starting on the last one', async () => {
  rememberInstance('creative-flat');
  const { user, calls } = await setup({ name: 'settings' });
  await enter([d('Fresh.zip'), d('iris.jar')]);
  expect(await screen.findByText("You'll choose the instance after you drop.")).toBeInTheDocument();
  await drop([d('Fresh.zip'), d('iris.jar')]);
  const dialog = await screen.findByRole('dialog', { name: 'Add to which instance?' });
  expect(within(dialog).getByText('You dropped 1 mod and 1 resource pack.')).toBeInTheDocument();
  const select = within(dialog).getByRole('combobox', { name: 'Instance' });
  expect(select).toHaveValue('creative-flat');
  // A drag while the dialog is open is not taken.
  await enter([d('x.jar')]);
  expect(overlay()).not.toBeInTheDocument();
  await user.selectOptions(select, 'asalith-fields');
  await user.click(within(dialog).getByRole('button', { name: 'Add' }));
  await waitFor(() => expect(sent(calls, 'drop_add')).toEqual([{ id: 'asalith-fields', paths: [d('Fresh.zip'), d('iris.jar')] }]));
  expect(await screen.findByText('Added 1 mod and 1 resource pack to Asalith Fields')).toBeInTheDocument();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});

it('sends nothing when the player cancels the choice', async () => {
  const { user, calls } = await setup({ name: 'servers' });
  await enter([d('iris.jar')]);
  await drop([d('iris.jar')]);
  const dialog = await screen.findByRole('dialog', { name: 'Add to which instance?' });
  await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(calls, 'drop_add')).toEqual([]);
});

it('names what no instance takes without asking which one', async () => {
  const { calls } = await setup({ name: 'settings' });
  await enter([d('notes.txt')]);
  await drop([d('notes.txt')]);
  expect(await screen.findByText('Nothing was added')).toBeInTheDocument();
  expect(screen.getByText('Skipped: notes.txt (not a mod or pack)')).toBeInTheDocument();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(calls, 'drop_add')).toEqual([]);
});

it('takes the instance held over in the rail instead', async () => {
  const { calls } = await setup({ name: 'home', id: 'asalith-fields' });
  const cabinet = document.createElement('button');
  cabinet.dataset.instance = 'creative-flat';
  document.body.append(cabinet);
  document.elementFromPoint = vi.fn(() => cabinet);
  await enter([d('Fresh.zip')]);
  await over({ x: 20, y: 80 });
  const shown = await screen.findByTestId('drop-overlay');
  expect(within(shown).getByText('Creative Flat')).toBeInTheDocument();
  expect(cabinet).toHaveAttribute('data-drop-target');
  await drop([d('Fresh.zip')]);
  expect(cabinet).not.toHaveAttribute('data-drop-target');
  await waitFor(() => expect(sent(calls, 'drop_add')).toEqual([{ id: 'creative-flat', paths: [d('Fresh.zip')] }]));
  cabinet.remove();
  // @ts-expect-error jsdom has none of its own
  delete document.elementFromPoint;
});

it.each([
  ['a dialog is open', { name: 'home', id: 'asalith-fields' } as Route],
  ['Skins is open', { name: 'skins' } as Route],
  ['Manage is open', { name: 'manage', tenantId: 't', stack: 's' } as Route],
])('takes no drop while %s', async (why, route) => {
  const dialog = document.createElement('dialog');
  if (why === 'a dialog is open') {
    dialog.setAttribute('open', '');
    document.body.append(dialog);
  }
  const { calls } = await setup(route);
  await enter([d('a.jar')]);
  expect(overlay()).not.toBeInTheDocument();
  await drop([d('a.jar')]);
  expect(sent(calls, 'drop_add')).toEqual([]);
  expect(sent(calls, 'drop_inspect')).toEqual([]);
  dialog.remove();
});

it('tells the player when a drop arrives while the last one is still being added', async () => {
  const { calls, backend } = await setup({ name: 'home', id: 'asalith-fields' });
  const handle = backend.handle;
  let release = () => {};
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
    cmd === 'drop_add' ? new Promise((resolve) => (release = () => resolve(handle(cmd, args)))) : handle(cmd, args),
  );
  await drop([d('a.jar')]);
  await waitFor(() => expect(sent(calls, 'drop_add')).toHaveLength(1));
  await drop([d('b.jar')]);
  expect(await screen.findByText('Still adding the last files. Drop these again in a moment.')).toBeInTheDocument();
  expect(sent(calls, 'drop_add')).toHaveLength(1);
  await act(async () => release());
  expect(await screen.findByText('Added 1 mod to Asalith Fields')).toBeInTheDocument();
});

it('shows a failed add as an error', async () => {
  const { calls, backend } = await setup({ name: 'home', id: 'asalith-fields' });
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
    cmd === 'drop_add'
      ? Promise.reject({ category: 'disk', code: 'disk_write_failed', message: 'nope', fix: null, detail: null, params: {} })
      : handle(cmd, args),
  );
  await drop([d('a.jar')]);
  expect(await screen.findByText(/couldn.t|could not|write/i)).toBeInTheDocument();
  expect(calls.some(([cmd]) => cmd === 'drop_add')).toBe(true);
});

it('stops listening when it goes away', async () => {
  const { unmount } = await setup({ name: 'home', id: 'asalith-fields' });
  unmount();
  expect(unlisten).toHaveBeenCalledTimes(1);
});
