import { screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import type { Instance, RestorePoint } from '../../ipc/schema';
import { clearToasts, toastsSnapshot } from '../../state/toasts';
import { renderWithProviders } from '../../test/render';
import { GoBackDialog } from './GoBackDialog';

const instance = (): Instance => createMockBackend().state.listing.instances.find((i) => i.id === 'asalith-fields')!;

const point = (partial: Partial<RestorePoint> = {}): RestorePoint => ({
  kind: 'beforeChange',
  createdAt: 1791720000,
  changes: [{ kind: 'packUpdate', from: '1.4.1', to: '1.4.2' }],
  sizeBytes: 12_000_000,
  server: null,
  ...partial,
});

function renderDialog(p: RestorePoint, ipc: (cmd: string, args?: unknown) => unknown = () => null) {
  const onClose = vi.fn();
  const result = renderWithProviders(<GoBackDialog instance={instance()} point={p} onClose={onClose} />, { ipc });
  return { ...result, onClose, dialog: screen.getByRole('dialog', { name: 'Go back to how Asalith Fields was?' }) };
}

describe('GoBackDialog', () => {
  it('lists at most three changes, then how many more', () => {
    const { dialog } = renderDialog(
      point({
        changes: [
          { kind: 'packUpdate', from: '1.4.1', to: '1.4.2' },
          { kind: 'installed', name: 'Sodium' },
          { kind: 'installed', name: 'Lithium' },
          { kind: 'removed', name: 'JEI' },
          { kind: 'removed', name: 'Zoomify' },
        ],
      }),
    );
    expect(within(dialog).getByText('This undoes: updated to 1.4.2, installed Sodium, installed Lithium and 2 more')).toBeInTheDocument();
    expect(within(dialog).getByText('Your worlds, screenshots and game settings stay as they are.')).toBeInTheDocument();
  });

  it('lists every change when there are three or fewer', () => {
    const { dialog } = renderDialog(point({ changes: [{ kind: 'installed', name: 'Sodium' }, { kind: 'removed', name: 'JEI' }] }));
    expect(within(dialog).getByText('This undoes: installed Sodium and removed JEI')).toBeInTheDocument();
  });

  it('leaves out the undo line when the point names no changes', () => {
    const { dialog } = renderDialog(point({ kind: 'beforeGoingBack', changes: [] }));
    expect(within(dialog).queryByText(/This undoes/)).toBeNull();
  });

  it('shows the size from 1 MB, and hides it under that', () => {
    const big = renderDialog(point());
    expect(within(big.dialog).getByText('The restore point uses 12 MB.')).toBeInTheDocument();
    big.unmount();
    const small = renderDialog(point({ sizeBytes: 524_288 }));
    expect(within(small.dialog).queryByText(/restore point uses/)).toBeNull();
  });

  it('warns about the server only for a server instance', () => {
    const plain = renderDialog(point());
    expect(within(plain.dialog).queryByText(/uses the newer mods/)).toBeNull();
    plain.unmount();
    const server = renderDialog(point({ server: 'Survival' }));
    expect(
      within(server.dialog).getByText('Survival uses the newer mods. Until you update again, you may not be able to join.'),
    ).toBeInTheDocument();
  });

  it('goes back on Go back, then says so in a toast', async () => {
    clearToasts();
    const calls: [string, unknown][] = [];
    const { user, dialog, onClose } = renderDialog(point(), (cmd, args) => {
      calls.push([cmd, args]);
      return cmd === 'restore_go_back' ? point({ kind: 'beforeGoingBack', changes: [] }) : null;
    });
    await user.click(within(dialog).getByRole('button', { name: 'Go back' }));
    expect(calls.filter(([c]) => c === 'restore_go_back')).toEqual([['restore_go_back', { id: 'asalith-fields' }]]);
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(toastsSnapshot().map((t) => t.notice?.title)).toContain('Asalith Fields is back to how it was.');
  });

  it('toasts the error when going back fails', async () => {
    clearToasts();
    const { user, dialog } = renderDialog(point(), (cmd) => {
      if (cmd === 'restore_go_back') throw { category: 'game', code: 'instance_busy', message: 'busy', fix: null, detail: null, params: { id: 'asalith-fields' } };
      return null;
    });
    await user.click(within(dialog).getByRole('button', { name: 'Go back' }));
    await waitFor(() => expect(toastsSnapshot().some((t) => t.error)).toBe(true));
  });
});
