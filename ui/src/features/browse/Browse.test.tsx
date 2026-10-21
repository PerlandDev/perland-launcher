import { screen, waitFor, within } from '@testing-library/react';
import { expect, it } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import type { SearchRequest } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { Browse } from './Browse';

const searches = (calls: [string, unknown][]) =>
  sent(calls, 'browse_search').map((args) => (args as { request: SearchRequest }).request);
const searchbox = () => screen.getByRole('searchbox', { name: 'Search' });

it('shows the most downloaded content first and pages on', async () => {
  const { user, calls } = renderApp(<Browse route={{ name: 'browse', kind: 'mod' }} />);
  expect(await screen.findByRole('button', { name: /Sodium/ })).toBeInTheDocument();
  expect(searches(calls)[0]).toMatchObject({ query: '', kind: 'mod', sort: 'downloads', offset: 0, instance: null });
  expect(screen.getByText('27 results')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Load more' }));
  expect(await screen.findByText("That's everything")).toBeInTheDocument();
  expect(searches(calls).at(-1)?.offset).toBe(20);
});

it('waits for typing to settle before searching', async () => {
  const { user, calls } = renderApp(<Browse route={{ name: 'browse', kind: 'mod' }} />);
  await screen.findByRole('button', { name: /Sodium/ });
  await user.type(searchbox(), 'iris');
  await waitFor(() => expect(searches(calls).at(-1)).toMatchObject({ query: 'iris', sort: 'relevance' }));
  expect(searches(calls).filter((r) => r.query === 'i' || r.query === 'ir' || r.query === 'iri')).toHaveLength(0);
  expect(await screen.findByRole('button', { name: /Iris/ })).toBeInTheDocument();
});

it('scopes to an instance: its game, no modpacks, installed lamps', async () => {
  renderApp(<Browse route={{ name: 'browse', kind: 'mod', instance: 'asalith-fields' }} />);
  expect(await screen.findByText(/Adding to Asalith Fields · Fabric 1\.20\.1/)).toBeInTheDocument();
  expect(screen.queryByRole('tab', { name: 'Modpacks' })).not.toBeInTheDocument();
  expect(screen.queryByLabelText('Minecraft')).not.toBeInTheDocument();
  const row = (await screen.findByRole('button', { name: /Fabric API/ })).closest('li')!;
  expect(within(row).getByText('Installed')).toBeInTheDocument();
});

it('offers every kind globally and switches between them', async () => {
  const { user } = renderApp(<Browse route={{ name: 'browse', kind: 'mod' }} />);
  const tabs = await screen.findAllByRole('tab');
  expect(tabs.map((tab) => tab.textContent)).toEqual(['Modpacks', 'Mods', 'Resource packs', 'Shaders']);
  expect(screen.getByRole('tab', { name: 'Mods' })).toHaveAttribute('aria-selected', 'true');
  await user.click(screen.getByRole('tab', { name: 'Shaders' }));
  expect(routeSnapshot()).toMatchObject({ name: 'browse', kind: 'shader' });
});

it('shows the offline and rate-limited states', async () => {
  const { user } = renderApp(<Browse route={{ name: 'browse', kind: 'mod' }} />);
  await user.type(await screen.findByRole('searchbox', { name: 'Search' }), 'offline');
  expect(await screen.findByText("You're offline. Browse needs the internet.")).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  await user.clear(searchbox());
  await user.type(searchbox(), 'busy');
  expect(await screen.findByText('Too many searches. Trying again in 3 s')).toBeInTheDocument();
  expect(await screen.findByText('Too many searches. Trying again in 2 s', {}, { timeout: 2500 })).toBeInTheDocument();
});

it('says when nothing matches and clears the filters', async () => {
  const { user } = renderApp(<Browse route={{ name: 'browse', kind: 'mod' }} />);
  await user.type(await screen.findByRole('searchbox', { name: 'Search' }), 'zzz');
  expect(await screen.findByText('Nothing found for “zzz”')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Clear filters' }));
  expect(await screen.findByRole('button', { name: /Sodium/ })).toBeInTheDocument();
  expect(searchbox()).toHaveValue('');
});

it('focuses search with / and opens a project', async () => {
  const { user } = renderApp(<Browse route={{ name: 'browse', kind: 'mod' }} />);
  const sodium = await screen.findByRole('button', { name: /Sodium/ });
  await user.keyboard('/');
  expect(searchbox()).toHaveFocus();
  await user.click(sodium);
  expect(routeSnapshot()).toMatchObject({ name: 'browse', kind: 'mod', project: 'sodium' });
});

it('filters by category', async () => {
  const { user, calls } = renderApp(<Browse route={{ name: 'browse', kind: 'mod' }} />);
  await user.click(await screen.findByRole('button', { name: 'Library', pressed: false }));
  await waitFor(() => expect(searches(calls).at(-1)?.categories).toEqual(['library']));
  expect(await screen.findByRole('button', { name: /Fabric API/ })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /Sodium/ })).not.toBeInTheDocument();
});

it('keeps the first page and offers a retry when a later page fails', async () => {
  const backend = createMockBackend();
  let failPage2 = true;
  const { user } = renderWithProviders(<Browse route={{ name: 'browse', kind: 'mod' }} />, {
    ipc: (cmd, args) => {
      const offset = (args as { request?: SearchRequest } | undefined)?.request?.offset;
      if (cmd === 'browse_search' && offset === 20 && failPage2) {
        throw { category: 'network', code: 'host_unreachable', message: 'x', fix: null, detail: null, params: { host: 'api.modrinth.com' } };
      }
      return backend.handle(cmd, args);
    },
  });
  expect(await screen.findByRole('button', { name: /Sodium/ })).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Load more' }));
  expect(await screen.findByText("You're offline. Browse needs the internet.")).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /Sodium/ })).toBeInTheDocument();
  failPage2 = false;
  await user.click(screen.getByRole('button', { name: 'Load more' }));
  expect(await screen.findByText("That's everything")).toBeInTheDocument();
  expect(screen.queryByText("You're offline. Browse needs the internet.")).not.toBeInTheDocument();
});

it('never offers modpacks while the scope is still loading', () => {
  renderApp(<Browse route={{ name: 'browse', kind: 'mod', instance: 'asalith-fields' }} />);
  expect(screen.queryByRole('tab', { name: 'Modpacks' })).not.toBeInTheDocument();
});

it('leaves / alone when a modifier is held', async () => {
  const { user } = renderApp(<Browse route={{ name: 'browse', kind: 'mod' }} />);
  await screen.findByRole('button', { name: /Sodium/ });
  await user.keyboard('{Control>}/{/Control}');
  expect(searchbox()).not.toHaveFocus();
});

it('falls back to 10 s when the wait is unreadable', async () => {
  const backend = createMockBackend();
  renderWithProviders(<Browse route={{ name: 'browse', kind: 'mod' }} />, {
    ipc: (cmd, args) => {
      if (cmd === 'browse_search') {
        throw { category: 'network', code: 'browse_rate_limited', message: 'x', fix: null, detail: null, params: { seconds: 'soon' } };
      }
      return backend.handle(cmd, args);
    },
  });
  expect(await screen.findByText('Too many searches. Trying again in 10 s')).toBeInTheDocument();
});

it('goes back to Home when it was opened from the rail', async () => {
  const { user } = renderApp(<Browse route={{ name: 'browse', kind: 'mod' }} />);
  await user.click(await screen.findByRole('button', { name: 'Back' }));
  expect(routeSnapshot()).toEqual({ name: 'home', id: null });
});
