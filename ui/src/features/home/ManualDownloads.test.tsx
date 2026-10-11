import { QueryClientProvider } from '@tanstack/react-query';
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

it('after an update-only run, Continue updates again instead of playing', async () => {
  const blocked = [...files, { ...files[0]!, path: 'mods/b.jar', name: 'B', fileName: 'b.jar' }];
  const { user, calls } = renderApp(<ManualDownloads instance={sky} files={blocked} updateOnly pollMs={20} />, { manual: true });
  const dialog = await screen.findByRole('dialog', { name: 'Download these manually' });
  await vi.waitFor(() => expect(within(dialog).getByText('Found 2 of 2')).toBeInTheDocument());
  await user.click(within(dialog).getByRole('button', { name: 'Continue' }));
  expect(sent(calls, 'pack_update')).toEqual([{ id: 'sky-islands' }]);
  expect(sent(calls, 'play')).toEqual([]);
});

it('a later blocked run does not start from the last dialog’s "all found"', async () => {
  let answer: () => Promise<ManualProgress> = () => Promise.resolve({ found: 1, total: 1, remaining: [] });
  const { qc, rerender } = renderWithProviders(<ManualDownloads instance={sky} files={files} pollMs={20} />, {
    ipc: (cmd) => (cmd === 'manual_import' ? answer() : null),
  });
  const first = await screen.findByRole('dialog', { name: 'Download these manually' });
  await vi.waitFor(() => expect(within(first).getByRole('button', { name: 'Continue' })).toBeEnabled());
  // The dialog closes; some time later another run is blocked on the same file.
  rerender(<QueryClientProvider client={qc}>{null}</QueryClientProvider>);
  await new Promise((resolve) => setTimeout(resolve, 20));
  let reply!: (progress: ManualProgress) => void;
  answer = () => new Promise((resolve) => (reply = resolve));
  rerender(
    <QueryClientProvider client={qc}>
      <ManualDownloads instance={sky} files={files} pollMs={20} />
    </QueryClientProvider>,
  );
  const second = await screen.findByRole('dialog', { name: 'Download these manually' });
  expect(within(second).getByRole('button', { name: 'Continue' })).toBeDisabled();
  expect(within(second).getByText('Found 0 of 1')).toBeInTheDocument();
  await vi.waitFor(() => expect(reply).toBeTypeOf('function'));
  reply({ found: 1, total: 1, remaining: [] });
  await vi.waitFor(() => expect(within(second).getByRole('button', { name: 'Continue' })).toBeEnabled());
});
