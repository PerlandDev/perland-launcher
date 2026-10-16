import { act, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { createMockBackend, MOCK_IMPORT_PATH } from '../../ipc/mock';
import type { Instance, LauncherError } from '../../ipc/schema';
import { handleEvent } from '../../state/handleEvent';
import { navigate, routeSnapshot } from '../../state/route';
import { sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ImportForm } from './ImportForm';

const TOTAL = 1_234_000_000;

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
  expect(screen.getByText(/In your other launcher, open the instance's folder/)).toBeInTheDocument();
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
    screen.getByText("We'll copy 1.2 GB: your mods, worlds, settings and map data. The original folder isn't changed."),
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
  expect(routeSnapshot()).toEqual({ name: 'newInstance' });
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

it('cancels the copy when the player leaves the form', async () => {
  const { user, calls, unmount } = renderImport();
  await choose(user);
  await screen.findByRole('combobox', { name: 'Loader version' });
  await user.click(screen.getByRole('button', { name: 'Import' }));
  expect(sent(calls, 'import_cancel')).toHaveLength(0);
  unmount();
  expect(sent(calls, 'import_cancel')).toHaveLength(1);
});
