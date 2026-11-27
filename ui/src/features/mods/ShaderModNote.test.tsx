import { act, screen, waitFor, within } from '@testing-library/react';
import { expect, it } from 'vitest';
import { applyLanguage } from '../../i18n';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { Browse } from '../browse/Browse';
import { Mods } from './Mods';

const MISSING = "Shaders run through Iris, and this instance doesn't have it yet.";
const HINT = 'Turn it on in Options › Video Settings › Shader Packs.';

it('offers the shader mod on a modded instance without one, and adds it with its dependencies', async () => {
  const { user, calls } = renderApp(<Mods id="asalith-fields" tab="shader" />);
  expect(await screen.findByText(MISSING)).toBeInTheDocument();
  await user.click(await screen.findByRole('button', { name: 'Add Iris' }));
  expect(sent(calls, 'browse_plan')).toEqual([
    { request: { project: 'iris', version: null, target: { kind: 'instance', id: 'asalith-fields' } } },
  ]);
  // Iris needs Sodium, so the usual confirm lists both.
  const dialog = await screen.findByRole('dialog');
  expect(within(dialog).getByRole('heading', { name: 'Add Iris to Asalith Fields' })).toBeInTheDocument();
  expect(within(dialog).getByText(/Sodium/)).toBeInTheDocument();
  await user.click(within(dialog).getByRole('button', { name: 'Install' }));
  await waitFor(() => expect(screen.queryByText(MISSING)).not.toBeInTheDocument());
  expect(sent(calls, 'browse_install')).toHaveLength(1);
  expect(sent(calls, 'shader_support').length).toBeGreaterThan(1);
  // With Iris there, the game has its Shader Packs menu again.
  expect(await screen.findByText(HINT)).toBeInTheDocument();
});

it('drops the Shader Packs hint while the shader mod is missing', async () => {
  renderApp(<Mods id="asalith-fields" tab="shader" />);
  await screen.findByText(MISSING);
  expect(screen.queryByText(HINT)).not.toBeInTheDocument();
});

it('shows no note when the pack already brings the shader mod', async () => {
  const { calls } = renderApp(<Mods id="sky-islands" tab="shader" />);
  await waitFor(() => expect(sent(calls, 'shader_support')).toEqual([{ id: 'sky-islands' }]));
  expect(await screen.findByText(HINT)).toBeInTheDocument();
  expect(screen.queryByText(/Shaders run through/)).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /^Add (Iris|Oculus)$/ })).not.toBeInTheDocument();
});

it('leaves vanilla to the loader note', async () => {
  renderApp(<Mods id="creative-flat" tab="shader" />);
  expect(await screen.findByText('Shaders need a mod loader: Fabric, Quilt, Forge or NeoForge.')).toBeInTheDocument();
  expect(screen.queryByText(/Shaders run through/)).not.toBeInTheDocument();
});

it("says so plainly when the mod has no version for the instance, and opens its page instead", async () => {
  const { user, backend } = renderApp(<Mods id="asalith-fields" tab="shader" />);
  const handle = backend.handle;
  backend.handle = (cmd, args) => (cmd === 'browse_versions' ? Promise.resolve([]) : handle(cmd, args));
  expect(await screen.findByText("Shaders run through Iris, but Iris doesn't have a version for this instance yet.")).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Add Iris' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: "Open Iris's page" }));
  expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'mod', instance: 'asalith-fields', project: 'iris' });
});

it('adds the mod while the game runs, like any other mod', async () => {
  const { user, calls } = renderApp(<Mods id="asalith-fields" tab="shader" />);
  const add = await screen.findByRole('button', { name: 'Add Iris' });
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  expect(add).not.toHaveAttribute('aria-disabled');
  await user.click(add);
  await waitFor(() => expect(sent(calls, 'browse_plan')).not.toEqual([]));
});

it('shows a failed add as an error card on the note, and keeps the note', async () => {
  const { user, backend } = renderApp(<Mods id="asalith-fields" tab="shader" />);
  const handle = backend.handle;
  backend.handle = (cmd, args) =>
    cmd === 'browse_plan'
      ? Promise.reject({ category: 'network', code: 'host_unreachable', message: "Couldn't reach the server.", fix: null, detail: null, params: {} })
      : handle(cmd, args);
  await user.click(await screen.findByRole('button', { name: 'Add Iris' }));
  expect(await screen.findByRole('alert')).toBeInTheDocument();
  expect(screen.getByText(MISSING)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Add Iris' })).not.toHaveAttribute('aria-disabled');
});

it('offers the shader mod above shader results in a scoped Browse, and not for other kinds', async () => {
  renderApp(<Browse route={{ name: 'browse', kind: 'shader', instance: 'asalith-fields' }} />);
  expect(await screen.findByText(MISSING)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Add Iris' })).toBeInTheDocument();
});

it('keeps the shader note out of a scoped mods Browse', async () => {
  const { calls } = renderApp(<Browse route={{ name: 'browse', kind: 'mod', instance: 'asalith-fields' }} />);
  await screen.findByRole('tab', { name: 'Mods' });
  expect(sent(calls, 'shader_support')).toEqual([]);
  expect(screen.queryByText(MISSING)).not.toBeInTheDocument();
});

it('reads in Spanish', async () => {
  applyLanguage('es');
  renderApp(<Mods id="asalith-fields" tab="shader" />);
  expect(await screen.findByText('Los shaders funcionan con Iris, y esta instancia todavía no lo tiene.')).toBeInTheDocument();
  expect(await screen.findByRole('button', { name: 'Agregar Iris' })).toBeInTheDocument();
});
