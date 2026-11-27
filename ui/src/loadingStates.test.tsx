// Every screen that waits on data says so (after a moment), with its frame and title already up, instead of
// showing an empty area that reads as "nothing happened".
import { screen, waitFor } from '@testing-library/react';
import type { ReactElement } from 'react';
import { expect, it } from 'vitest';
import { Home } from './features/home/Home';
import { InstanceSettings } from './features/instance-settings/InstanceSettings';
import { Personalize } from './features/personalize/Personalize';
import { Settings } from './features/settings/Settings';
import { createMockBackend } from './ipc/mock';
import { renderWithProviders } from './test/render';

/** Renders against the mock backend, with `waiting` commands that never answer. */
function renderWaiting(ui: ReactElement, waiting: string[]) {
  const backend = createMockBackend();
  return renderWithProviders(ui, { ipc: (cmd, args) => (waiting.includes(cmd) ? new Promise(() => {}) : backend.handle(cmd, args)) });
}

it('Home says it is getting the instances', async () => {
  renderWaiting(<Home id={null} />, ['instances_list']);
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Getting your instances…'));
});

it('Settings keeps its title up and says it is getting the settings', async () => {
  renderWaiting(<Settings />, ['settings_get']);
  expect(screen.getByRole('heading', { name: 'Settings', level: 1 })).toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Getting your settings…'));
});

it('Personalize keeps its title up and says it is getting the settings', async () => {
  renderWaiting(<Personalize />, ['settings_get']);
  expect(screen.getByRole('heading', { name: 'Personalize', level: 1 })).toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Getting your settings…'));
});

it("Instance settings keeps its title up and says it is getting the instance's settings", async () => {
  renderWaiting(<InstanceSettings id="asalith" />, ['app_info']);
  expect(screen.getByRole('heading', { name: 'Instance settings', level: 1 })).toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent("Getting this instance's settings…"));
});
