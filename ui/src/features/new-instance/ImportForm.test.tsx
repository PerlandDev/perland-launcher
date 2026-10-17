import { act, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { createMockBackend, MOCK_IMPORT_PATH } from '../../ipc/mock';
import type { Instance, LauncherError } from '../../ipc/schema';
import { handleEvent } from '../../state/handleEvent';
import { importRunningSnapshot } from '../../state/importProgress';
import { navigate, routeSnapshot } from '../../state/route';
import { toastsSnapshot } from '../../state/toasts';
import { sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ImportForm } from './ImportForm';

const TOTAL = 1_234_000_000;
/** A preview of a folder whose launcher kept no settings of the instance's own. */
const NOTHING_KEPT = { memoryMb: null, memoryWantedMb: null, javaArgs: null, playtimeSeconds: null };

const notInstance: LauncherError = {
  category: 'disk',
  code: 'import_not_instance',
  message: "This folder doesn't look like a Minecraft instance.",
  fix: null,
  detail: null,
  params: {},
};

const cancelled: LauncherError = { ...notInstance, code: 'import_cancelled', message: 'The import was cancelled.' };

function progress(done: number) {
  act(() => handleEvent({ type: 'progress', instance: null, task: { key: 'import_instance', params: {} }, done, total: TOTAL, bytes: done }));
}

/** The mock backend, with `import_instance` held open until the test settles it. */
function renderImport(overrides: Record<string, (args: unknown) => unknown> = {}) {
  const backend = createMockBackend();
  const calls: [string, unknown][] = [];
  let settle: { resolve: (value: unknown) => void; reject: (error: unknown) => void } | null = null;
  const result = renderWithProviders(<ImportForm />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      if (overrides[cmd]) return overrides[cmd](args);
      if (cmd === 'import_instance') return new Promise((resolve, reject) => (settle = { resolve, reject }));
      return backend.handle(cmd, args);
    },
  });
  return {
    ...result,
    backend,
    calls,
    settle: () => {
      if (!settle) throw new Error('import_instance was not called');
      return settle;
    },
  };
}

async function choose(user: ReturnType<typeof renderImport>['user']) {
  await user.click(await screen.findByRole('button', { name: 'Choose folder…' }));
  return screen.findByRole('textbox', { name: 'Name' });
}

it('reads the picked folder and prefills the form from it', async () => {
  const { user, calls } = renderImport();
  expect(screen.getByText(/In your other launcher, open the instance's folder.*Close the game in your other launcher first\./)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();

  const name = await choose(user);

  expect(sent(calls, 'plugin:dialog|open')).toEqual([
    { options: expect.objectContaining({ directory: true, multiple: false, title: "Pick the instance's folder" }) },
  ]);
  expect(sent(calls, 'import_inspect')).toEqual([{ path: MOCK_IMPORT_PATH }]);
  expect(name).toHaveValue('Skyblock Adventures');
  await screen.findByRole('option', { name: '26.3' });
  expect(screen.getByRole('combobox', { name: 'Minecraft version' })).toHaveValue('1.20.1');
  expect(screen.getByRole('radio', { name: 'Fabric' })).toBeChecked();
  expect(await screen.findByRole('combobox', { name: 'Loader version' })).toHaveValue('0.19.4');
  expect(
    screen.getByText(
      "We'll copy 1.2 GB: your mods, worlds, settings and map data. The original folder isn't changed. " +
        'It keeps its memory (6 GB) and its Java arguments. Its 12 h 30 min played count toward your time played.',
    ),
  ).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Import' })).toBeEnabled();
});

it('imports with the edited name, shows the copy, then opens the new instance', async () => {
  const { user, calls, backend, settle } = renderImport();
  const name = await choose(user);
  await screen.findByRole('combobox', { name: 'Loader version' });
  await user.clear(name);
  await user.type(name, 'Skyblock');
  await user.click(screen.getByRole('button', { name: 'Import' }));

  expect(sent(calls, 'import_instance')).toEqual([
    {
      request: { path: MOCK_IMPORT_PATH, name: 'Skyblock', minecraft: '1.20.1', loader: { kind: 'fabric', version: '0.19.4' } },
    },
  ]);
  progress(617_000_000);
  expect(screen.getByRole('progressbar', { name: 'Copying… 617 MB of 1.2 GB' })).toHaveAttribute('aria-valuenow', '50');
  expect(screen.getByText('Copying… 617 MB of 1.2 GB')).toBeInTheDocument();
  expect(screen.getByRole('textbox', { name: 'Name' })).toBeDisabled();

  progress(TOTAL);
  const created = (await backend.handle('instance_create', {
    request: { name: 'Skyblock', minecraft: '1.20.1', loader: { kind: 'fabric', version: '0.19.4' } },
  })) as Instance;
  await act(async () => settle().resolve(created));

  await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: created.id }));
});

it('cancels a running import and goes back to the form quietly', async () => {
  navigate({ name: 'newInstance' });
  const { user, calls, settle } = renderImport();
  await choose(user);
  await screen.findByRole('combobox', { name: 'Loader version' });
  await user.click(screen.getByRole('button', { name: 'Import' }));
  progress(100_000_000);

  await user.click(screen.getByRole('button', { name: 'Cancel' }));
  expect(sent(calls, 'import_cancel')).toHaveLength(1);
  await act(async () => settle().reject(cancelled));

  await waitFor(() => expect(screen.queryByRole('progressbar')).not.toBeInTheDocument());
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Import' })).toBeEnabled();
  expect(screen.getByRole('textbox', { name: 'Name' })).toBeEnabled();
  expect(screen.getByRole('button', { name: 'Cancel' })).not.toHaveAttribute('aria-disabled');
  expect(routeSnapshot()).toEqual({ name: 'newInstance' });
});

it('names the import after the folder around a bare .minecraft when nothing was detected', async () => {
  const path = 'C:\\Users\\p\\Profiles\\Survival\\.minecraft';
  const { user, calls, settle } = renderImport({
    'plugin:dialog|open': () => path,
    import_inspect: () => ({ path, name: null, minecraft: null, loader: null, sizeBytes: 1000, ...NOTHING_KEPT }),
  });
  const name = await choose(user);
  expect(name).toHaveValue('');
  expect(name).toHaveAttribute('placeholder', 'Survival');
  await screen.findByRole('option', { name: '26.3' });

  await user.click(screen.getByRole('button', { name: 'Import' }));
  expect(sent(calls, 'import_instance')).toEqual([{ request: expect.objectContaining({ path, name: 'Survival', loader: null }) }]);
  settle();
});

it('explains a folder that is not an instance, and lets the player choose again', async () => {
  let inspected = 0;
  const backend = createMockBackend();
  const { user, calls } = renderImport({
    import_inspect: (args) => {
      inspected += 1;
      if (inspected === 1) throw notInstance;
      return backend.handle('import_inspect', args);
    },
  });

  await user.click(await screen.findByRole('button', { name: 'Choose folder…' }));

  const alert = await screen.findByRole('alert');
  expect(alert).toHaveTextContent("This folder doesn't look like a Minecraft instance.");
  expect(alert).toHaveTextContent("Pick the instance's own folder: the one with mods, saves or config inside.");
  expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();

  await choose(user);
  expect(sent(calls, 'import_inspect')).toHaveLength(2);
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('keeps the import running when the player leaves, and shows its progress on coming back', async () => {
  const first = renderImport();
  await choose(first.user);
  await screen.findByRole('combobox', { name: 'Loader version' });
  await first.user.click(screen.getByRole('button', { name: 'Import' }));
  progress(100_000_000);
  first.unmount();
  expect(sent(first.calls, 'import_cancel')).toHaveLength(0);
  progress(617_000_000);

  // Back to New instance, From another launcher: the copy, not the pick step.
  const back = renderImport();
  expect(screen.getByRole('progressbar', { name: 'Copying… 617 MB of 1.2 GB' })).toHaveAttribute('aria-valuenow', '50');
  expect(screen.getByText('Skyblock Adventures')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Choose folder…' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Importing…' })).toBeDisabled();
  expect(sent(back.calls, 'import_instance')).toHaveLength(0);

  await back.user.click(screen.getByRole('button', { name: 'Cancel' }));
  expect(sent(back.calls, 'import_cancel')).toHaveLength(1);
  expect(screen.getByRole('button', { name: 'Cancel' })).toHaveAttribute('aria-disabled', 'true');
  await act(async () => first.settle().reject(cancelled));

  await waitFor(() => expect(screen.queryByRole('progressbar')).not.toBeInTheDocument());
  expect(screen.getByRole('button', { name: 'Choose folder…' })).toBeEnabled();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  expect(toastsSnapshot()).toEqual([]);
});

it('opens the new instance when an import the player came back to finishes', async () => {
  const first = renderImport();
  await choose(first.user);
  await screen.findByRole('combobox', { name: 'Loader version' });
  await first.user.click(screen.getByRole('button', { name: 'Import' }));
  first.unmount();
  renderImport();
  const created = (await first.backend.handle('instance_create', {
    request: { name: 'Skyblock Adventures', minecraft: '1.20.1', loader: { kind: 'fabric', version: '0.19.4' } },
  })) as Instance;
  await act(async () => first.settle().resolve(created));

  await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: created.id }));
  expect(toastsSnapshot()).toEqual([]);
});

it('says the instance is ready when the import finishes while the player is elsewhere', async () => {
  navigate({ name: 'newInstance' });
  const { user, backend, unmount, settle } = renderImport();
  await choose(user);
  await screen.findByRole('combobox', { name: 'Loader version' });
  await user.click(screen.getByRole('button', { name: 'Import' }));
  unmount();
  navigate({ name: 'settings' });
  const created = (await backend.handle('instance_create', {
    request: { name: 'Skyblock Adventures', minecraft: '1.20.1', loader: { kind: 'fabric', version: '0.19.4' } },
  })) as Instance;
  await act(async () => settle().resolve(created));

  await vi.waitFor(() => expect(toastsSnapshot()).toEqual([expect.objectContaining({ notice: { title: 'Skyblock Adventures is ready' } })]));
  expect(routeSnapshot()).toEqual({ name: 'settings' });
  expect(importRunningSnapshot()).toBeNull();
});

it('reports a failure while the player is elsewhere as a toast, but never a cancel', async () => {
  const failed: LauncherError = { ...notInstance, code: 'not_enough_space', message: 'Not enough free space.', params: { needed_mb: '900', free_mb: '10' } };
  for (const [error, toasts] of [
    [cancelled, 0],
    [failed, 1],
  ] as const) {
    const { user, unmount, settle } = renderImport();
    await choose(user);
    await screen.findByRole('combobox', { name: 'Loader version' });
    await user.click(screen.getByRole('button', { name: 'Import' }));
    unmount();
    await act(async () => settle().reject(error));
    await vi.waitFor(() => expect(importRunningSnapshot()).toBeNull());
    expect(toastsSnapshot()).toHaveLength(toasts);
  }
  expect(toastsSnapshot()[0]?.error).toEqual(expect.objectContaining({ code: 'not_enough_space' }));
});

it('drops the old import failure when another folder is chosen', async () => {
  const failed: LauncherError = { ...notInstance, code: 'not_enough_space', message: 'Not enough free space.', params: { needed_mb: '900', free_mb: '10' } };
  let answer: (value: unknown) => void = () => {};
  const backend = createMockBackend();
  let inspected = 0;
  const { user, settle } = renderImport({
    import_inspect: (args) => {
      inspected += 1;
      if (inspected === 1) return backend.handle('import_inspect', args);
      return new Promise((resolve) => {
        answer = () => resolve(backend.handle('import_inspect', args));
      });
    },
  });
  await choose(user);
  await screen.findByRole('combobox', { name: 'Loader version' });
  await user.click(screen.getByRole('button', { name: 'Import' }));
  await act(async () => settle().reject(failed));
  expect(await screen.findByRole('alert')).toHaveTextContent('Not enough free space: 900 MB needed, 10 MB free.');

  await user.click(screen.getByRole('button', { name: 'Choose folder…' }));
  expect(await screen.findByRole('status')).toHaveTextContent('Reading the folder…');
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();

  await act(async () => answer(null));
  expect(await screen.findByRole('textbox', { name: 'Name' })).toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('says it is reading the folder while it looks at it', async () => {
  let answer: (value: unknown) => void = () => {};
  const backend = createMockBackend();
  const { user } = renderImport({
    import_inspect: (args) =>
      new Promise((resolve) => {
        answer = () => resolve(backend.handle('import_inspect', args));
      }),
  });
  await user.click(await screen.findByRole('button', { name: 'Choose folder…' }));

  expect(await screen.findByRole('status')).toHaveTextContent('Reading the folder…');
  expect(screen.getByRole('button', { name: 'Choose folder…' })).toBeDisabled();
  await act(async () => answer(null));
  expect(await screen.findByRole('textbox', { name: 'Name' })).toHaveValue('Skyblock Adventures');
  expect(screen.queryByText('Reading the folder…')).not.toBeInTheDocument();
});

it('cuts a long folder name or detected name to the 64 characters a name can have', async () => {
  const long = 'A'.repeat(70);
  const path = `/home/p/instances/${long}`;
  const first = renderImport({
    'plugin:dialog|open': () => path,
    import_inspect: () => ({ path, name: null, minecraft: null, loader: null, sizeBytes: 1000, ...NOTHING_KEPT }),
  });
  const name = await choose(first.user);
  expect(name).toHaveAttribute('placeholder', 'A'.repeat(64));
  await screen.findByRole('option', { name: '26.3' });
  await first.user.click(screen.getByRole('button', { name: 'Import' }));
  expect(sent(first.calls, 'import_instance')).toEqual([{ request: expect.objectContaining({ name: 'A'.repeat(64) }) }]);
  await act(async () => first.settle().reject(cancelled));
  first.unmount();

  const second = renderImport({
    'plugin:dialog|open': () => path,
    import_inspect: () => ({ path, name: 'B'.repeat(70), minecraft: null, loader: null, sizeBytes: 1000, ...NOTHING_KEPT }),
  });
  expect(await choose(second.user)).toHaveValue('B'.repeat(64));
});

it('says which of its own settings the instance keeps', async () => {
  const preview = { path: MOCK_IMPORT_PATH, name: 'Asalith Fields', minecraft: '1.20.1', loader: null, sizeBytes: 1000 };
  const { user } = renderImport({
    import_inspect: () => ({
      ...preview,
      memoryMb: 10240,
      memoryWantedMb: null,
      javaArgs: '-XX:+UseG1GC',
      playtimeSeconds: 7_500,
    }),
  });
  await choose(user);
  expect(
    screen.getByText(
      "We'll copy 1 KB: your mods, worlds, settings and map data. The original folder isn't changed. " +
        'It keeps its memory (10 GB) and its Java arguments. Its 2 h 5 min played count toward your time played.',
    ),
  ).toBeInTheDocument();
});

it('says when the memory it had is more than the launcher allows', async () => {
  const { user } = renderImport({
    import_inspect: () => ({
      path: MOCK_IMPORT_PATH,
      name: 'Big',
      minecraft: '1.20.1',
      loader: null,
      sizeBytes: 1000,
      memoryMb: 65_536,
      memoryWantedMb: 100_000,
      javaArgs: null,
      playtimeSeconds: null,
    }),
  });
  await choose(user);
  expect(screen.getByText(/It had 97\.7 GB of memory, more than the launcher allows, so it gets 64 GB\./)).toBeInTheDocument();
});
