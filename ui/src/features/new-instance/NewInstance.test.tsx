import { screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import type { LoaderVersion } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { NewInstance } from './NewInstance';

it('creates a Fabric instance with the newest release and stable loader by default', async () => {
  const { user, calls } = renderApp(<NewInstance />);
  const version = await screen.findByRole('combobox', { name: 'Minecraft version' });
  await screen.findByRole('option', { name: '26.3' });
  expect(version).toHaveValue('26.3');
  expect(screen.queryByRole('option', { name: '26.4-snapshot-3' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('radio', { name: 'Fabric' }));
  expect(await screen.findByRole('combobox', { name: 'Loader version' })).toHaveValue('0.19.5');
  await user.click(screen.getByRole('button', { name: 'Create instance' }));
  expect(sent(calls, 'instance_create')).toEqual([
    { request: { name: 'Minecraft 26.3', minecraft: '26.3', loader: { kind: 'fabric', version: '0.19.5' } } },
  ]);
  await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'minecraft-26-3' }));
});

it('offers Forge and NeoForge and creates a Forge instance', async () => {
  const { user, calls } = renderApp(<NewInstance />);
  await screen.findByRole('option', { name: '26.3' });
  expect(screen.getByRole('radio', { name: 'NeoForge' })).toBeInTheDocument();
  await user.click(screen.getByRole('radio', { name: 'Forge' }));
  expect(await screen.findByRole('combobox', { name: 'Loader version' })).toHaveValue('0.19.5');
  await user.click(screen.getByRole('button', { name: 'Create instance' }));
  expect(sent(calls, 'instance_create')).toEqual([
    { request: { name: 'Minecraft 26.3', minecraft: '26.3', loader: { kind: 'forge', version: '0.19.5' } } },
  ]);
});

it('shows snapshots on request and keeps pack imports as coming soon', async () => {
  const { user } = renderApp(<NewInstance />);
  await screen.findByRole('option', { name: '26.3' });
  await user.click(screen.getByRole('switch', { name: 'Show snapshots' }));
  expect(screen.getByRole('option', { name: '26.4-snapshot-3' })).toBeInTheDocument();
  expect(screen.getByRole('combobox', { name: 'Minecraft version' })).toHaveValue('26.3');
  expect(screen.getByText('Paste a pack link').closest('[aria-disabled]')).toHaveAttribute('aria-disabled', 'true');
  expect(screen.getByText('Import a .mrpack file').closest('[aria-disabled]')).toHaveAttribute('aria-disabled', 'true');
});

/** The mock backend, with the catalog calls overridden per test. */
function renderWithCatalog(catalog: {
  loaders?: (mc: string) => LoaderVersion[];
  minecraft?: () => unknown;
}) {
  const backend = createMockBackend();
  return renderWithProviders(<NewInstance />, {
    ipc: (cmd, args) => {
      if (cmd === 'catalog_loaders' && catalog.loaders) return catalog.loaders((args as { mc: string }).mc);
      if (cmd === 'catalog_minecraft' && catalog.minecraft) return catalog.minecraft();
      return backend.handle(cmd, args);
    },
  });
}

it('starts from the newest stable loader again after the Minecraft version changes', async () => {
  const { user } = renderWithCatalog({
    loaders: (mc) =>
      mc === '1.21.1'
        ? [
            { version: '0.16.14', stable: true },
            { version: '0.17.0-beta.1', stable: false },
          ]
        : [
            { version: '0.19.5', stable: true },
            { version: '0.19.4', stable: true },
          ],
  });
  await screen.findByRole('option', { name: '26.3' });
  await user.click(screen.getByRole('radio', { name: 'Fabric' }));
  const loaderVersion = await screen.findByRole('combobox', { name: 'Loader version' });
  await user.selectOptions(loaderVersion, '0.19.4');
  expect(loaderVersion).toHaveValue('0.19.4');
  await user.selectOptions(screen.getByRole('combobox', { name: 'Minecraft version' }), '1.21.1');
  await waitFor(() => expect(screen.getByRole('combobox', { name: 'Loader version' })).toHaveValue('0.16.14'));
});

it('tells the player what to do when the loader has no build for the chosen version', async () => {
  const { user } = renderWithCatalog({ loaders: (mc) => (mc === '1.20.1' ? [] : [{ version: '0.19.5', stable: true }]) });
  await screen.findByRole('option', { name: '1.20.1' });
  await user.click(screen.getByRole('radio', { name: 'Quilt' }));
  await user.selectOptions(screen.getByRole('combobox', { name: 'Minecraft version' }), '1.20.1');
  expect(await screen.findByText(/Quilt isn't available for this Minecraft version yet. Pick another Minecraft version/)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Create instance' })).toBeDisabled();
});

it('offers Try again when the version list fails to load', async () => {
  let attempts = 0;
  const { user } = renderWithCatalog({
    minecraft: () => {
      attempts += 1;
      if (attempts === 1) throw { category: 'network', code: 'network_unreachable', message: 'No connection', fix: null, detail: null, params: {} };
      return createMockBackend().handle('catalog_minecraft', undefined);
    },
  });
  expect(await screen.findByRole('alert')).toHaveTextContent('No connection');
  expect(screen.getByRole('button', { name: 'Create instance' })).toBeDisabled();
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  expect(await screen.findByRole('option', { name: '26.3' })).toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('uses a plain name hint until the version list has loaded', async () => {
  renderApp(<NewInstance />);
  const name = screen.getByRole('textbox', { name: 'Name' });
  expect(name).toHaveAttribute('placeholder', 'My instance');
  await screen.findByRole('option', { name: '26.3' });
  expect(name).toHaveAttribute('placeholder', 'Minecraft 26.3');
});
