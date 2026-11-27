import { act, fireEvent, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { renderWithProviders } from '../../test/render';
import { SHOW_AFTER_MS } from '../../ui/ScreenLoading';

// The studio's chunk (three.js) is the slow part: here it is a promise the test settles.
const load = vi.hoisted(() => vi.fn<() => Promise<{ default: (props: object) => React.ReactNode }>>());
vi.mock('./loadSkinStudio', () => ({ loadSkinStudio: load, prefetchSkinStudio: () => {} }));

// The screen keeps its lazy studio between visits, so each test takes a fresh copy of the module.
async function renderScreen() {
  vi.resetModules();
  const { SkinsScreen } = await import('./SkinsScreen');
  return renderWithProviders(<SkinsScreen />);
}

beforeEach(() => {
  load.mockReset();
  vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it('shows the real header at once while the studio is on its way, then says what is loading', async () => {
  load.mockReturnValue(new Promise(() => {}));
  vi.useFakeTimers();
  await renderScreen();
  // The player sees the click worked: the back key, the title and the studio's tabs, which wait for the studio.
  expect(screen.getByRole('heading', { name: 'Skins', level: 1 })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Back' })).toBeInTheDocument();
  const tabs = within(screen.getByRole('tablist')).getAllByRole('tab');
  expect(tabs.map((tab) => tab.textContent)).toEqual(['Create', 'Paint', 'Catalog']);
  for (const tab of tabs) expect(tab).toHaveAttribute('aria-disabled', 'true');
  // Paint is where the studio opens, so it is the one pressed in, as it will be.
  expect(screen.getByRole('tab', { name: 'Paint' })).toHaveAttribute('aria-selected', 'true');
  act(() => void vi.advanceTimersByTime(SHOW_AFTER_MS));
  expect(screen.getByRole('status')).toHaveTextContent('Setting up the skin studio…');
});

it('shows an error card under the header when the studio fails to load, and Try again loads it afresh', async () => {
  load.mockRejectedValueOnce(new Error('Failed to fetch dynamically imported module: /assets/SkinStudio-abc123.js'));
  await renderScreen();
  const alert = await screen.findByRole('alert');
  expect(alert).toHaveTextContent("The skin studio didn't open.");
  expect(alert).toHaveTextContent('Try again. If it keeps happening, restart the launcher.');
  // The way back is still there.
  expect(screen.getByRole('button', { name: 'Back' })).toBeInTheDocument();
  load.mockResolvedValueOnce({ default: () => <p>The studio</p> });
  fireEvent.click(within(alert).getByRole('button', { name: 'Try again' }));
  expect(await screen.findByText('The studio')).toBeInTheDocument();
  expect(load).toHaveBeenCalledTimes(2);
});

it('hands the studio the skin and account it was opened for', async () => {
  const studio = vi.fn((props: object) => <p>{JSON.stringify(props)}</p>);
  load.mockResolvedValue({ default: studio });
  vi.resetModules();
  const { SkinsScreen } = await import('./SkinsScreen');
  renderWithProviders(<SkinsScreen skin="s1" account="a1" />);
  expect(await screen.findByText('{"skin":"s1","account":"a1"}')).toBeInTheDocument();
});
