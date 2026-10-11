import { screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import manualFx from '../../ipc/contract/manual_progress.json';
import { createMockBackend } from '../../ipc/mock';
import type { Instance, ManualFile, ManualProgress } from '../../ipc/schema';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ManualDownloads } from './ManualDownloads';

const files = (manualFx as ManualProgress).remaining as ManualFile[];
const sky = { id: 'sky-islands', name: 'Sky Islands' } as Instance;

it('opens each CurseForge page through the opener', async () => {
  const backend = createMockBackend();
  const calls: [string, unknown][] = [];
  const { user } = renderWithProviders(<ManualDownloads instance={sky} files={files} pollMs={20} />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === 'manual_import') return { found: 0, total: files.length, remaining: files };
      return backend.handle(cmd, args);
    },
  });
  const dialog = await screen.findByRole('dialog', { name: 'Download these manually' });
  expect(within(dialog).getByText('Found 0 of 1')).toBeInTheDocument();
  await user.click(within(dialog).getByRole('button', { name: 'Open page' }));
  expect(calls.some(([cmd, args]) => cmd === 'plugin:opener|open_url' && (args as { url: string }).url === files[0]!.pageUrl)).toBe(true);
  expect(within(dialog).getByRole('button', { name: 'Continue' })).toBeDisabled();
});

it('enables Continue once every file is found, and plays again', async () => {
  const blocked = [...files, { ...files[0]!, path: 'mods/b.jar', name: 'B', fileName: 'b.jar' }];
  const { user, calls } = renderApp(<ManualDownloads instance={sky} files={blocked} pollMs={20} />, { manual: true });
  const dialog = await screen.findByRole('dialog', { name: 'Download these manually' });
  await vi.waitFor(() => expect(within(dialog).getByText('Found 2 of 2')).toBeInTheDocument());
  await user.click(within(dialog).getByRole('button', { name: 'Continue' }));
  expect(sent(calls, 'play')).toEqual([{ id: 'sky-islands' }]);
});
