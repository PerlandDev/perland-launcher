import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import { applyContentProgress, contentInstalling, contentProgressSnapshot } from '../../state/contentProgress';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { Toasts } from '../../ui/Toasts';
import type { VersionInfo } from '../../ipc/schema';
import { Browse } from './Browse';

const page = (project: string, instance?: string) => (
  <>
    <Browse route={{ name: 'browse', kind: 'mod', project, instance }} />
    <Toasts />
  </>
);

it('confirms a mod that brings a dependency, then installs it', async () => {
  const { user, calls } = renderApp(page('iris', 'asalith-fields'));
  await user.click(await screen.findByRole('button', { name: 'Install' }));
  const dialog = await screen.findByRole('dialog', { name: 'Add Iris to Asalith Fields' });
  expect(within(dialog).getByText(/Sodium/)).toBeInTheDocument();
  expect(within(dialog).getByText('needed by Iris')).toBeInTheDocument();
  expect(within(dialog).getByText('Already have: Fabric API')).toBeInTheDocument();
  expect(sent(calls, 'browse_plan')).toEqual([
    { request: { project: 'YL57xq9U', version: 'YL57xq9U-v1', target: { kind: 'instance', id: 'asalith-fields' } } },
  ]);
  await user.click(within(dialog).getByRole('button', { name: 'Install' }));
  expect(await screen.findByText('Added Iris + 1 more to Asalith Fields')).toBeInTheDocument();
  expect(sent(calls, 'browse_install')).toEqual([{ planId: 'plan-1' }]);
  expect(await screen.findByText('Already in Asalith Fields')).toBeInTheDocument();
});

it('installs straight away when nothing else comes along', async () => {
  const { user, calls } = renderApp(
    <>
      <Browse route={{ name: 'browse', kind: 'resource_pack', project: 'faithful', instance: 'asalith-fields' }} />
      <Toasts />
    </>,
  );
  await user.click(await screen.findByRole('button', { name: 'Install' }));
  expect(await screen.findByText('Added Faithful to Asalith Fields')).toBeInTheDocument();
  expect(screen.getByText('Turn it on in Options › Resource Packs.')).toBeInTheDocument();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(sent(calls, 'browse_install')).toHaveLength(1);
});

it('disables instances that cannot run it, with the reason', async () => {
  renderApp(page('sodium'));
  const picker = await screen.findByRole('combobox', { name: 'Install to' });
  // The reasons appear once the project's versions have loaded.
  expect(await within(picker).findByRole('option', { name: 'Creative Flat — Needs a mod loader' })).toBeDisabled();
  expect(within(picker).getByRole('option', { name: 'Sky Islands — No version for this instance' })).toBeDisabled();
  expect(within(picker).getByRole('option', { name: 'Asalith Fields' })).toBeEnabled();
  expect(picker).toHaveValue('asalith-fields');
});

it('makes a new instance and goes to it', async () => {
  const { user } = renderApp(page('sodium'));
  const picker = await screen.findByRole('combobox', { name: 'Install to' });
  await user.selectOptions(picker, 'New instance');
  await user.click(screen.getByRole('button', { name: 'Install' }));
  const dialog = await screen.findByRole('dialog', { name: /^New instance: Sodium · Fabric/ });
  await user.click(within(dialog).getByRole('button', { name: 'Install' }));
  await waitFor(() => expect(routeSnapshot()).toMatchObject({ name: 'home', id: 'sodium' }));
});

it('creates an instance from a modpack', async () => {
  const { user, calls } = renderApp(<Browse route={{ name: 'browse', kind: 'modpack', project: 'fabulously-optimized' }} />);
  await user.click(await screen.findByRole('button', { name: 'Create instance' }));
  await waitFor(() =>
    expect(sent(calls, 'pack_add')).toEqual([{ request: { kind: 'link', url: 'modrinth:1KVo5zza@1KVo5zza-v1' } }]),
  );
  await waitFor(() => expect(routeSnapshot()).toMatchObject({ name: 'home' }));
});

it('installs from a scoped result row', async () => {
  const { user } = renderApp(<Browse route={{ name: 'browse', kind: 'resource_pack', instance: 'asalith-fields' }} />);
  const row = (await screen.findByRole('button', { name: /Faithful/ })).closest('li')!;
  await user.click(within(row).getByRole('button', { name: 'Install' }));
  expect(await within(row).findByText('Installed')).toBeInTheDocument();
});

it('leaves no progress meter behind when an install fails', async () => {
  const backend = createMockBackend();
  const { user } = renderWithProviders(page('faithful', 'asalith-fields'), {
    ipc: (cmd, args) => {
      if (cmd === 'browse_install') {
        // The core got partway, reported progress, then failed.
        applyContentProgress({
          type: 'progress',
          instance: 'asalith-fields',
          task: { key: 'install_content', params: { name: 'Faithful' } },
          done: 1,
          total: 3,
          bytes: 10,
        });
        return backend.handle(cmd, { planId: 'plan-gone' });
      }
      return backend.handle(cmd, args);
    },
  });
  await user.click(await screen.findByRole('button', { name: 'Install' }));
  expect(await screen.findByText('That took too long. Try installing again.')).toBeInTheDocument();
  expect(contentProgressSnapshot('asalith-fields')).toBeNull();
  expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  expect(contentInstalling()).toBe(false);
});

it('counts as an install in flight (Restart now waits) only while browse_install runs', async () => {
  const backend = createMockBackend();
  const during: boolean[] = [];
  const { user } = renderWithProviders(page('faithful', 'asalith-fields'), {
    ipc: (cmd, args) => {
      if (cmd === 'browse_install') during.push(contentInstalling());
      return backend.handle(cmd, args);
    },
  });
  expect(contentInstalling()).toBe(false);
  await user.click(await screen.findByRole('button', { name: 'Install' }));
  expect(await screen.findByText('Added Faithful to Asalith Fields')).toBeInTheDocument();
  expect(during).toEqual([true]);
  expect(contentInstalling()).toBe(false);
});

const unreachable = {
  category: 'network',
  code: 'host_unreachable',
  message: "Couldn't reach api.modrinth.com",
  fix: null,
  detail: null,
  params: { host: 'api.modrinth.com' },
};

it('shows a failed version list with a retry, and no wrong reasons', async () => {
  const backend = createMockBackend();
  let failures = 1;
  const { user } = renderWithProviders(page('sodium'), {
    ipc: (cmd, args) => {
      if (cmd === 'browse_versions' && failures-- > 0) throw unreachable;
      return backend.handle(cmd, args);
    },
  });
  expect(await screen.findByRole('alert')).toBeInTheDocument();
  expect(screen.queryByText(/No version for this instance/)).not.toBeInTheDocument();
  expect(screen.queryByText('No version fits this instance.')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Install' })).toBeDisabled();
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  const picker = await screen.findByRole('combobox', { name: 'Install to' });
  expect(await within(picker).findByRole('option', { name: 'Sky Islands — No version for this instance' })).toBeDisabled();
  expect(screen.getByRole('combobox', { name: 'Version' })).toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('keeps Create instance disabled when a modpack has no version list', async () => {
  const backend = createMockBackend();
  renderWithProviders(<Browse route={{ name: 'browse', kind: 'modpack', project: 'fabulously-optimized' }} />, {
    ipc: (cmd, args) => {
      if (cmd === 'browse_versions') throw unreachable;
      return backend.handle(cmd, args);
    },
  });
  expect(await screen.findByRole('alert')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Create instance' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
});

it('installs into what the picker shows when the route names an unknown instance', async () => {
  const { user, calls } = renderApp(page('sodium', 'long-gone'));
  const picker = await screen.findByRole('combobox', { name: 'Install to' });
  await waitFor(() => expect(picker).toHaveValue('asalith-fields'));
  const button = screen.getByRole('button', { name: 'Install' });
  await waitFor(() => expect(button).toBeEnabled());
  await user.click(button);
  await waitFor(() => expect(sent(calls, 'browse_plan')).toHaveLength(1));
  expect(sent(calls, 'browse_plan')).toMatchObject([{ request: { target: { kind: 'instance', id: 'asalith-fields' } } }]);
});

it('cannot be cancelled while the install is running', async () => {
  const backend = createMockBackend();
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => (release = resolve));
  const { user } = renderWithProviders(page('iris', 'asalith-fields'), {
    ipc: async (cmd, args) => {
      if (cmd === 'browse_install') await gate;
      return backend.handle(cmd, args);
    },
  });
  await user.click(await screen.findByRole('button', { name: 'Install' }));
  const dialog = await screen.findByRole('dialog', { name: 'Add Iris to Asalith Fields' });
  await user.click(within(dialog).getByRole('button', { name: 'Install' }));
  await waitFor(() => expect(within(dialog).getByRole('button', { name: 'Cancel' })).toBeDisabled());
  fireEvent(dialog, new Event('cancel', { cancelable: true }));
  expect(screen.getByRole('dialog', { name: 'Add Iris to Asalith Fields' })).toBeInTheDocument();
  release();
  expect(await screen.findByText('Added Iris + 1 more to Asalith Fields')).toBeInTheDocument();
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
});

it('does not call the install failed when only showing the result goes wrong', async () => {
  const backend = createMockBackend();
  const log = vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(runStore, 'run').mockImplementation(() => {
    throw new Error('boom');
  });
  const { user } = renderWithProviders(page('faithful', 'asalith-fields'), { ipc: (cmd, args) => backend.handle(cmd, args) });
  await user.click(await screen.findByRole('button', { name: 'Install' }));
  await waitFor(() => expect(log).toHaveBeenCalled());
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  vi.restoreAllMocks();
});

it('starts a new instance from the newest version the core can make one from', async () => {
  const backend = createMockBackend();
  const sodium = (await backend.handle('browse_versions', { project: 'AANobbMI' })) as VersionInfo[];
  const base = sodium[0]!;
  const versions: VersionInfo[] = [
    // The newest is a server plugin build, which no instance can run.
    { ...base, id: 'paper-build', number: '9.9', versionType: 'release', date: '2026-09-01T00:00:00Z', gameVersions: ['1.21.1'], loaders: ['bukkit', 'paper'] },
    { ...base, id: 'snapshot-only', number: '9.8', versionType: 'release', date: '2026-08-01T00:00:00Z', gameVersions: ['24w14a'], loaders: ['fabric'] },
    { ...base, id: 'fabric-build', number: '1.0', versionType: 'release', date: '2026-01-01T00:00:00Z', gameVersions: ['1.21.1'], loaders: ['fabric'] },
  ];
  const calls: [string, unknown][] = [];
  const { user } = renderWithProviders(page('sodium'), {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === 'browse_versions') return versions;
      return backend.handle(cmd, args);
    },
  });
  const picker = await screen.findByRole('combobox', { name: 'Install to' });
  await user.selectOptions(picker, 'New instance');
  const version = screen.getByRole('combobox', { name: 'Version' });
  await waitFor(() => expect(within(version).getAllByRole('option')).toHaveLength(1));
  await user.click(screen.getByRole('button', { name: 'Install' }));
  await waitFor(() => expect(sent(calls, 'browse_plan')).toHaveLength(1));
  expect(sent(calls, 'browse_plan')).toMatchObject([{ request: { version: 'fabric-build', target: { kind: 'new' } } }]);
});

it('shows the instance a failed new-instance install left behind', async () => {
  const backend = createMockBackend();
  const calls: [string, unknown][] = [];
  const { user } = renderWithProviders(page('sodium'), {
    ipc: async (cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === 'browse_install') {
        // The core made the instance, then the install itself failed.
        await backend.handle('instance_create', { request: { name: 'Sodium', minecraft: '1.20.1', loader: null } });
        throw unreachable;
      }
      return backend.handle(cmd, args);
    },
  });
  const picker = await screen.findByRole('combobox', { name: 'Install to' });
  await user.selectOptions(picker, 'New instance');
  await user.click(screen.getByRole('button', { name: 'Install' }));
  const dialog = await screen.findByRole('dialog', { name: /^New instance: Sodium/ });
  const before = sent(calls, 'instances_list').length;
  await user.click(within(dialog).getByRole('button', { name: 'Install' }));
  await waitFor(() => expect(sent(calls, 'instances_list').length).toBeGreaterThan(before));
});
