import { act, screen, waitFor, within } from '@testing-library/react';
import { expect, it } from 'vitest';
import { applyLanguage } from '../../i18n';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { Browse } from '../browse/Browse';
import { Home } from '../home/Home';
import { Mods } from './Mods';

it('Home opens "Mods & packs" on the Mods tab', async () => {
  const { user } = renderApp(<Home id="asalith-fields" />);
  await user.click(await screen.findByRole('button', { name: 'Mods & packs' }));
  expect(routeSnapshot()).toEqual({ name: 'mods', id: 'asalith-fields' });
});

it('shows three tabs with Mods selected, and switching keeps the route linkable', async () => {
  const { user } = renderApp(<Mods id="asalith-fields" />);
  expect(await screen.findByRole('heading', { name: 'Mods & packs' })).toBeInTheDocument();
  const tabs = screen.getAllByRole('tab');
  expect(tabs.map((tab) => tab.textContent)).toEqual(['Mods', 'Resource packs', 'Shaders']);
  expect(screen.getByRole('tab', { name: 'Mods' })).toHaveAttribute('aria-selected', 'true');
  expect(await screen.findByText('sodium-fabric-0.5.8+mc1.20.1')).toBeInTheDocument();
  await user.click(screen.getByRole('tab', { name: 'Shaders' }));
  expect(routeSnapshot()).toEqual({ name: 'mods', id: 'asalith-fields', tab: 'shader' });
  await user.click(screen.getByRole('tab', { name: 'Mods' }));
  expect(routeSnapshot()).toEqual({ name: 'mods', id: 'asalith-fields' });
});

it('lists resource packs, removes one after confirming, and keeps folders', async () => {
  const { user, calls } = renderApp(<Mods id="asalith-fields" tab="resource_pack" />);
  expect(await screen.findByText('Faithful 32x')).toBeInTheDocument();
  expect(screen.getByRole('tab', { name: 'Resource packs' })).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByText('3 MB')).toBeInTheDocument();
  expect(screen.getByText('Turn it on in Options › Resource Packs.')).toBeInTheDocument();
  const folder = screen.getByText('My Pack').closest('li')!;
  expect(within(folder).getByText('Folder')).toBeInTheDocument();
  expect(within(folder).queryByRole('button')).not.toBeInTheDocument();
  expect(screen.getByRole('heading', { name: 'Your packs' })).toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Remove Faithful 32x' }));
  const dialog = screen.getByRole('dialog', { name: 'Remove Faithful 32x?' });
  expect(within(dialog).getByText('The file is deleted from this instance.')).toBeInTheDocument();
  await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
  expect(sent(calls, 'content_remove')).toEqual([{ id: 'asalith-fields', kind: 'resource_pack', file: 'Faithful 32x.zip' }]);
  await waitFor(() => expect(screen.queryByText('Faithful 32x')).not.toBeInTheDocument());
  expect(screen.getByText('My Pack')).toBeInTheDocument();
});

it('finds resource packs for this instance and opens the folder', async () => {
  const { user, calls } = renderApp(<Mods id="asalith-fields" tab="resource_pack" />);
  await screen.findByText('Faithful 32x');
  await user.click(screen.getByRole('button', { name: 'Open folder' }));
  expect(sent(calls, 'open_folder')).toEqual([{ target: { kind: 'resource_packs', id: 'asalith-fields' } }]);
  await user.click(screen.getByRole('button', { name: 'Find resource packs' }));
  expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'resource_pack', instance: 'asalith-fields' });
});

it('lists shaders and opens the shader folder', async () => {
  const { user, calls } = renderApp(<Mods id="asalith-fields" tab="shader" />);
  expect(await screen.findByText('ComplementaryReimagined')).toBeInTheDocument();
  expect(screen.getByText('Turn it on in Options › Video Settings › Shader Packs.')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Open folder' }));
  expect(sent(calls, 'open_folder')).toEqual([{ target: { kind: 'shader_packs', id: 'asalith-fields' } }]);
  await user.click(screen.getByRole('button', { name: 'Find shaders' }));
  expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'shader', instance: 'asalith-fields' });
});

it("shows a pack's own files read-only under From the pack", async () => {
  renderApp(<Mods id="sky-islands" tab="resource_pack" />);
  expect(await screen.findByText('Pack Textures')).toBeInTheDocument();
  expect(screen.getByRole('heading', { name: 'From the pack' })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /^Remove/ })).not.toBeInTheDocument();
});

it('says so when there are no resource packs', async () => {
  renderApp(<Mods id="creative-flat" tab="resource_pack" />);
  expect(await screen.findByText(/No resource packs yet/)).toBeInTheDocument();
});

it('tells vanilla players shaders need a loader, with no Find shaders button', async () => {
  renderApp(<Mods id="creative-flat" tab="shader" />);
  expect(await screen.findByText('Shaders need a mod loader: Fabric, Quilt, Forge or NeoForge.')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Find shaders' })).not.toBeInTheDocument();
});

it('keeps Remove off while the game is running', async () => {
  renderApp(<Mods id="asalith-fields" tab="resource_pack" />);
  await screen.findByText('Faithful 32x');
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(await screen.findByText('Stop the game to remove packs.')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Remove Faithful 32x' })).toHaveAttribute('aria-disabled', 'true');
});

it('shows a refused removal as a toast and keeps the file listed', async () => {
  const { user, backend } = renderApp(<Mods id="asalith-fields" tab="resource_pack" />);
  const handle = backend.handle;
  backend.handle = (cmd, args) =>
    cmd === 'content_remove'
      ? Promise.reject({ category: 'game', code: 'instance_busy', message: 'The game is open.', fix: null, detail: null, params: {} })
      : handle(cmd, args);
  await user.click(await screen.findByRole('button', { name: 'Remove Faithful 32x' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  await waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
  expect(toastsSnapshot().at(0)?.error).toMatchObject({ code: 'instance_busy' });
  expect(screen.getByText('Faithful 32x')).toBeInTheDocument();
});

it('adds a Browse install to the instance list', async () => {
  const { backend } = renderApp(<Mods id="asalith-fields" tab="resource_pack" />);
  const plan = (await backend.handle('browse_plan', {
    request: { project: 'FAITHFUL', version: null, target: { kind: 'instance', id: 'asalith-fields' } },
  })) as { planId: string };
  await backend.handle('browse_install', { planId: plan.planId });
  const list = (await backend.handle('content_list', { id: 'asalith-fields', kind: 'resource_pack' })) as { fileName: string }[];
  expect(list.map((f) => f.fileName)).toContain('faithful.zip');
});

it('goes back from a scoped Browse to the tab of its kind', async () => {
  const { user } = renderApp(<Browse route={{ name: 'browse', kind: 'resource_pack', instance: 'asalith-fields' }} />);
  await user.click(await screen.findByRole('button', { name: 'Back' }));
  expect(routeSnapshot()).toEqual({ name: 'mods', id: 'asalith-fields', tab: 'resource_pack' });
});

it('goes back from a scoped mods Browse to the Mods tab', async () => {
  const { user } = renderApp(<Browse route={{ name: 'browse', kind: 'mod', instance: 'asalith-fields' }} />);
  await user.click(await screen.findByRole('button', { name: 'Back' }));
  expect(routeSnapshot()).toEqual({ name: 'mods', id: 'asalith-fields' });
});

it('reads in Spanish', async () => {
  applyLanguage('es');
  renderApp(<Mods id="asalith-fields" tab="resource_pack" />);
  expect(await screen.findByRole('heading', { name: 'Mods y packs' })).toBeInTheDocument();
  expect(screen.getByRole('tab', { name: 'Paquetes de recursos' })).toBeInTheDocument();
  expect(await screen.findByRole('button', { name: 'Buscar paquetes de recursos' })).toBeInTheDocument();
  expect(await screen.findByRole('button', { name: 'Quitar Faithful 32x' })).toBeInTheDocument();
});
