import type { ReactElement } from 'react';
import { createMockBackend, type MockOptions } from '../ipc/mock';
import { renderWithProviders } from './render';

/** Render against the fixture-seeded mock backend and record every command sent to it. */
export function renderApp(ui: ReactElement, options: MockOptions = {}) {
  const backend = createMockBackend(options);
  const calls: [string, unknown][] = [];
  const result = renderWithProviders(ui, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      return backend.handle(cmd, args);
    },
  });
  return { ...result, backend, calls };
}

export const sent = (calls: [string, unknown][], cmd: string) => calls.filter(([c]) => c === cmd).map(([, args]) => args);
