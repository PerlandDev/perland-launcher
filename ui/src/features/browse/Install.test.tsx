import { screen, waitFor, within } from '@testing-library/react';
import { expect, it } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import { applyContentProgress, contentProgressSnapshot } from '../../state/contentProgress';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { Toasts } from '../../ui/Toasts';
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
});
