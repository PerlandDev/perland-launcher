import { QueryClientProvider } from '@tanstack/react-query';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { render } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactElement } from 'react';
import { createQueryClient } from '../state/queries';

export type IpcHandler = (cmd: string, args?: unknown) => unknown;

/** Render with a fresh query client and a mocked Rust side (events mocked too). */
export function renderWithProviders(ui: ReactElement, { ipc }: { ipc?: IpcHandler } = {}) {
  mockWindows('main');
  mockIPC((cmd, args) => (ipc ? ipc(cmd, args) : null), { shouldMockEvents: true });
  const qc = createQueryClient();
  const user = userEvent.setup();
  return { qc, user, ...render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>) };
}
