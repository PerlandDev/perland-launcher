import { screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import { createMockBackend } from '../../ipc/mock';
import { MOCK_INVITE, MOCK_SERVER_ICON } from '../../ipc/mockServers';
import type { NewsPostView, ServerNews } from '../../ipc/schema';
import { toastsSnapshot } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { Home } from '../home/Home';
import { ServersScreen } from '../servers-list/ServersScreen';

const B_INVITE = `https://play.example.com/${'B'.repeat(43)}`;
/** The mock's posts are relative to this moment: a Friday, 6 PM UTC (the tests run in UTC). */
const NOW = new Date('2026-10-16T18:00:00Z');
const HOUR = 3_600_000;

const newsPost = (over: Partial<NewsPostView> & Pick<NewsPostView, 'id' | 'title' | 'createdAt'>): NewsPostView => ({
  type: 'update',
  body: '',
  startsAt: null,
  endsAt: null,
  imageVersion: null,
  modpack: null,
  ...over,
});

/** The card (an article) named by its server's name. */
const card = async (name: string) => (await screen.findByRole('heading', { name })).closest('article')!;
/** A card's news line: the button whose name starts "News:". */
const newsLine = (scope: HTMLElement, name: RegExp = /^News: /) => within(scope).findByRole('button', { name });

beforeEach(() => {
  // Only the clock: timers stay real, so the queries and user-event run as usual.
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(NOW);
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('the news line', () => {
  it('a card shows the latest news line', async () => {
    renderApp(<ServersScreen />);
    // Survival's newest post is a picture, but its upcoming event wins: it's what a player can still act on.
    const survival = await card('Survival');
    const line = await newsLine(survival);
    expect(line).toHaveAccessibleName(/^News: Build contest · Tomorrow 8\sPM/);
    expect(line).toHaveTextContent(/Build contest · Tomorrow 8\sPM/);
    // An event under way reads "Now"; one three days ahead reads its weekday.
    expect(await newsLine(await card('Creative Plots'))).toHaveAccessibleName(/^News: Plot party · Now/);
    expect(await newsLine(await card('The Whitelist Club'))).toHaveAccessibleName(/^News: Build contest · Mon 6\sPM/);
    // A plain post reads its title.
    expect(await newsLine(await card('Skyblock Dawn'))).toHaveAccessibleName('News: Islands reset on Friday');
    // A server without posts, and one without an invite, show no line at all.
    await newsLine(await card('Aventura Épica'));
    expect(within(await card('Sleepy Hollow')).queryByRole('button', { name: /^News:/ })).toBeNull();
    expect(within(await card('Tech Forge')).queryByRole('button', { name: /^News:/ })).toBeNull();
  });

  it('the dot shows for unseen news and clears after opening', async () => {
    const { user, calls } = renderApp(<ServersScreen />);
    const survival = await card('Survival');
    const line = await newsLine(survival, /, new$/);
    expect(within(line).getByTitle('New')).toBeInTheDocument();
    // Skyblock Dawn's only post is nine days old and was never opened: no wall of dots on a first visit.
    expect(await newsLine(await card('Skyblock Dawn'))).toHaveAccessibleName('News: Islands reset on Friday');

    await user.click(line);
    const dialog = await screen.findByRole('dialog', { name: 'News from Survival' });
    // The dialog lives outside the card, so the card's hover and focus rules (which light its Play) never match through it.
    expect(survival).not.toContainElement(dialog);
    // The marker moves to the newest post the dialog shows (two hours before now).
    await vi.waitFor(() => expect(sent(calls, 'server_news_opened')).toEqual([{ invite: MOCK_INVITE, newest: new Date(NOW.getTime() - 2 * HOUR).toISOString() }]));
    await user.click(within(dialog).getByRole('button', { name: 'Close' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    await vi.waitFor(() => expect(within(survival).getByRole('button', { name: /^News: / })).toHaveAccessibleName(/Tomorrow 8\sPM$/));
    expect(within(survival).queryByTitle('New')).toBeNull();
  });

  it('only cards on screen ask for news', async () => {
    // One card is on screen: Creative Plots. Every other observed element (other cards, the list's sentinel) is not.
    vi.stubGlobal(
      'IntersectionObserver',
      class {
        constructor(private readonly callback: IntersectionObserverCallback) {}
        observe(el: Element) {
          const name = el.closest('article')?.querySelector('h3')?.textContent;
          this.callback([{ isIntersecting: name === 'Creative Plots', target: el } as IntersectionObserverEntry], this as unknown as IntersectionObserver);
        }
        unobserve() {}
        disconnect() {}
      },
    );
    const { calls } = renderApp(<ServersScreen />);
    await newsLine(await card('Creative Plots'));
    await card('Survival');
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(sent(calls, 'server_news')).toEqual([{ invite: B_INVITE, before: null, limit: 20 }]);
    expect(within(await card('Survival')).queryByRole('button', { name: /^News:/ })).toBeNull();
  });

  it('nothing shows with news off', async () => {
    const { calls } = renderApp(<ServersScreen />, { server: 'news-off' });
    await card('Survival');
    await vi.waitFor(() => expect(sent(calls, 'server_news').length).toBeGreaterThan(3));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.queryByRole('button', { name: /^News:/ })).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(toastsSnapshot()).toEqual([]);
  });

  it('a failing read shows nothing either', async () => {
    const backend = createMockBackend();
    const calls: [string, unknown][] = [];
    renderWithProviders(<ServersScreen />, {
      ipc: (cmd, args) => {
        calls.push([cmd, args]);
        if (cmd === 'server_news') return Promise.reject({ category: 'network', code: 'offline', message: "You're offline", fix: null, detail: null, params: {} });
        return backend.handle(cmd, args);
      },
    });
    await card('Survival');
    await vi.waitFor(() => expect(sent(calls, 'server_news').length).toBeGreaterThan(3));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.queryByRole('button', { name: /^News:/ })).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(toastsSnapshot()).toEqual([]);
  });

  it('an event that is over shows no time on the line, and its date in the dialog', async () => {
    const backend = createMockBackend();
    // Survival's newest post is an event that ended at 2 PM, read at 6 PM; its other post has a time that does not parse.
    const page: ServerNews = {
      posts: [
        newsPost({ id: '41000000-0000-4000-8000-000000000001', type: 'event', title: 'Build contest', startsAt: '2026-10-16T12:00:00.000000Z', endsAt: '2026-10-16T14:00:00Z', createdAt: '2026-10-16T11:00:00.123456Z' }),
        newsPost({ id: '41000000-0000-4000-8000-000000000002', title: 'Odd post', createdAt: '' }),
      ],
      nextBefore: null,
      seen: '2026-10-16T11:00:00.123456Z',
    };
    const { user } = renderWithProviders(<ServersScreen />, {
      ipc: (cmd, args) => (cmd === 'server_news' ? Promise.resolve((args as { invite: string }).invite === MOCK_INVITE ? page : null) : backend.handle(cmd, args)),
    });
    const line = await newsLine(await card('Survival'));
    expect(line).toHaveAccessibleName('News: Build contest');
    expect(line).toHaveTextContent(/^Build contest$/);
    await user.click(line);
    const dialog = await screen.findByRole('dialog', { name: 'News from Survival' });
    expect(within(dialog).getByText(/^Oct 16, 12\sPM$/)).toBeInTheDocument();
    expect(within(dialog).queryByText(/^Today/)).toBeNull();
    // The post whose time does not parse still shows, without a Posted line.
    expect(within(dialog).getByRole('heading', { name: 'Odd post' })).toBeInTheDocument();
    expect(within(dialog).queryByText(/^Posted/)).toBeNull();
  });
});

describe('the news dialog', () => {
  it('the dialog lists posts, pages with Show more, and shows pictures', async () => {
    const { user, calls } = renderApp(<ServersScreen />);
    await user.click(await newsLine(await card('Survival')));
    const dialog = await screen.findByRole('dialog', { name: 'News from Survival' });
    const titles = () => within(dialog).getAllByRole('heading', { level: 3 }).map((h) => h.textContent);
    // Newest first: the line's page (20 posts) shows at once.
    expect(titles()).toHaveLength(20);
    expect(titles().slice(0, 3)).toEqual(['New spawn is live', 'Modpack updated', 'Build contest']);
    expect(within(dialog).getByText(/^Tomorrow 8\sPM$/)).toBeInTheDocument();
    expect(within(dialog).getByText('Posted Oct 16')).toBeInTheDocument();
    // The picture comes through the core, and the body keeps its line breaks as text.
    await vi.waitFor(() => expect(dialog.querySelector('img')).toHaveAttribute('src', MOCK_SERVER_ICON));
    expect(sent(calls, 'news_image')).toEqual([{ invite: MOCK_INVITE, postId: expect.any(String), imageVersion: 'v1' }]);
    const body = within(dialog).getByText(/^Come and see it\./);
    expect(body.textContent).toBe('Come and see it.\nThe old spawn stays as a museum.');
    expect(body).toHaveClass('news-post-body'); // white-space: pre-line in app.css
    // The Close is ghost: nothing in the dialog is lit.
    expect(within(dialog).getByRole('button', { name: 'Close' })).toHaveClass('btn-ghost');
    expect(dialog.querySelector('.btn-primary')).toBeNull();

    await user.click(within(dialog).getByRole('button', { name: 'Show more' }));
    await vi.waitFor(() => expect(titles()).toHaveLength(24));
    expect(sent(calls, 'server_news').at(-1)).toEqual({ invite: MOCK_INVITE, before: '20', limit: 20 });
    expect(within(dialog).queryByRole('button', { name: 'Show more' })).toBeNull();
  });

  it('a failed Show more says so in the dialog, with Try again', async () => {
    const backend = createMockBackend();
    let fail = true;
    const { user } = renderWithProviders(<ServersScreen />, {
      ipc: (cmd, args) => {
        const before = (args as { before?: string | null } | undefined)?.before;
        if (cmd === 'server_news' && before && fail) return Promise.reject({ category: 'network', code: 'invite_rate_limited', message: 'Slow down', fix: null, detail: null, params: {} });
        return backend.handle(cmd, args);
      },
    });
    await user.click(await newsLine(await card('Survival')));
    const dialog = await screen.findByRole('dialog', { name: 'News from Survival' });
    await user.click(within(dialog).getByRole('button', { name: 'Show more' }));
    expect(await within(dialog).findByText("Couldn't load more news.")).toBeInTheDocument();
    expect(toastsSnapshot()).toEqual([]);
    fail = false;
    await user.click(within(dialog).getByRole('button', { name: 'Try again' }));
    await vi.waitFor(() => expect(within(dialog).getAllByRole('heading', { level: 3 })).toHaveLength(24));
    expect(within(dialog).queryByText("Couldn't load more news.")).toBeNull();
  });

  it('a body is plain text', async () => {
    const { user } = renderApp(<ServersScreen />);
    await user.click(await newsLine(await card('Survival')));
    const dialog = await screen.findByRole('dialog', { name: 'News from Survival' });
    expect(within(dialog).getByText('<b>bold</b>')).toBeInTheDocument();
    expect(dialog.querySelector('b')).toBeNull();
  });

  it("a modpack post reads in the player's language", async () => {
    await i18n.changeLanguage('es');
    const { user } = renderApp(<ServersScreen />);
    await user.click(await newsLine(await card('Survival'), /^Novedades: /));
    const dialog = await screen.findByRole('dialog', { name: 'Novedades de Survival' });
    expect(within(dialog).getAllByRole('heading', { name: 'Modpack actualizado' })).toHaveLength(2);
    expect(within(dialog).getByText('3 mods agregados, 1 quitado, 2 actualizados')).toBeInTheDocument();
    expect(within(dialog).getByText('Sodium, Iris Shaders, Lithium')).toBeInTheDocument();
    // A modpack post without changes says so plainly; the Engine's English fallback never shows.
    expect(within(dialog).getByText('El modpack cambió.')).toBeInTheDocument();
    expect(within(dialog).queryByText(/Modpack 1\.0\./)).toBeNull();
    expect(within(dialog).getByRole('button', { name: 'Cerrar' })).toBeInTheDocument();
  });
});

describe('news on Home', () => {
  it('Home shows the line for a server instance', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'online' });
    const line = await screen.findByRole('button', { name: /^News: Build contest · Tomorrow 8\sPM, new$/ });
    await user.click(line);
    const dialog = await screen.findByRole('dialog', { name: 'News from Survival' });
    expect(line.closest('.server-tile')).toBeNull();
    expect(dialog.parentElement).toBe(document.body);
    await user.click(within(dialog).getByRole('button', { name: 'Close' }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('Home shows no line with news off', async () => {
    const { calls } = renderApp(<Home id="survival" />, { server: 'news-off' });
    await screen.findByRole('heading', { name: 'Survival' });
    await vi.waitFor(() => expect(sent(calls, 'server_news')).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.queryByRole('button', { name: /^News:/ })).toBeNull();
    expect(toastsSnapshot()).toEqual([]);
  });

  it('a plain instance never asks for news', async () => {
    const { calls } = renderApp(<Home id="asalith-fields" />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Asalith Fields' });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(sent(calls, 'server_news')).toEqual([]);
    expect(screen.queryByRole('button', { name: /^News:/ })).toBeNull();
  });
});
