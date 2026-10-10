import { screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
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

it('shows snapshots on request and keeps pack imports as coming soon', async () => {
  const { user } = renderApp(<NewInstance />);
  await screen.findByRole('option', { name: '26.3' });
  await user.click(screen.getByRole('switch', { name: 'Show snapshots' }));
  expect(screen.getByRole('option', { name: '26.4-snapshot-3' })).toBeInTheDocument();
  expect(screen.getByText('Paste a pack link').closest('[aria-disabled]')).toHaveAttribute('aria-disabled', 'true');
  expect(screen.getByText('Import a .mrpack file').closest('[aria-disabled]')).toHaveAttribute('aria-disabled', 'true');
});
